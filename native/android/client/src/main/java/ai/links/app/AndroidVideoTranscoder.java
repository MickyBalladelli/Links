package ai.links.app;

import android.graphics.SurfaceTexture;
import android.media.MediaCodec;
import android.media.MediaCodecInfo;
import android.media.MediaCodecList;
import android.media.MediaExtractor;
import android.media.MediaFormat;
import android.media.MediaMuxer;
import android.opengl.EGL14;
import android.opengl.EGLConfig;
import android.opengl.EGLContext;
import android.opengl.EGLDisplay;
import android.opengl.EGLExt;
import android.opengl.EGLSurface;
import android.opengl.GLES11Ext;
import android.opengl.GLES20;
import android.os.Build;
import android.view.Surface;
import java.io.File;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.FloatBuffer;

/**
 * Android hardware video transcoder. Decode and encode stay on MediaCodec
 * surfaces; EGL only scales the decoded frame into the encoder surface. The
 * muxed output contains the transcoded video and compatible AAC audio.
 */
public final class AndroidVideoTranscoder {
    public static final long MAX_INPUT_BYTES = 256L * 1024L * 1024L;
    private static final long TIMEOUT_US = 10_000L;
    private static final int EGL_RECORDABLE_ANDROID = 0x3142;

    public enum Codec {
        H264("video/avc"),
        HEVC("video/hevc");

        private final String mimeType;

        Codec(String mimeType) {
            this.mimeType = mimeType;
        }
    }

    public static final class Profile {
        public final Codec codec;
        public final int width;
        public final int height;
        public final int bitrateBps;
        public final int frameRate;

        private Profile(Codec codec, int width, int height, int bitrateBps) {
            this.codec = codec;
            this.width = width;
            this.height = height;
            this.bitrateBps = bitrateBps;
            this.frameRate = 30;
        }

        public static Profile hd720p(Codec codec) {
            return new Profile(codec, 1_280, 720, 1_500_000);
        }

        public static Profile fullHd1080p(Codec codec) {
            return new Profile(codec, 1_920, 1_080, 3_000_000);
        }

        private void validate() throws IOException {
            if (codec == null || bitrateBps <= 0 || frameRate <= 0
                    || !((width == 1_280 && height == 720)
                    || (width == 1_920 && height == 1_080))
                    || (width == 1_280 && bitrateBps != 1_500_000)
                    || (width == 1_920 && bitrateBps != 3_000_000)
                    || frameRate != 30)
                throw new IOException("Invalid video profile");
        }
    }

    public static final class TranscodedVideo {
        public final File file;
        public final Codec codec;
        public final int width;
        public final int height;
        public final long durationUs;
        public final boolean hasAudio;

        private TranscodedVideo(File file, Profile profile, long durationUs, boolean hasAudio) {
            this.file = file;
            this.codec = profile.codec;
            this.width = profile.width;
            this.height = profile.height;
            this.durationUs = durationUs;
            this.hasAudio = hasAudio;
        }
    }

    public TranscodedVideo transcode(File source, File destination, Profile profile)
            throws Exception {
        if (destination == null || destination.exists())
            throw new IOException("Invalid video destination");
        File parent = destination.getAbsoluteFile().getParentFile();
        if (parent == null || !parent.isDirectory())
            throw new IOException("Missing video destination directory");
        File staging = File.createTempFile(".links-video-", ".mp4", parent);
        if (!staging.delete()) throw new IOException("Cannot prepare video staging file");
        try {
            TranscodedVideo result = transcodeToFile(source, staging, profile);
            Mp4FastStart.rewrite(staging, destination);
            return new TranscodedVideo(destination, profile, result.durationUs, result.hasAudio);
        } finally {
            if (staging.exists()) staging.delete();
        }
    }

    private TranscodedVideo transcodeToFile(File source, File destination, Profile profile)
            throws Exception {
        if (source == null || destination == null || !source.isFile()
                || source.length() == 0 || source.length() > MAX_INPUT_BYTES
                || destination.exists())
            throw new IOException("Invalid video files");
        if (destination.getParentFile() != null && !destination.getParentFile().isDirectory())
            throw new IOException("Missing video destination directory");
        profile.validate();

        MediaExtractor videoExtractor = new MediaExtractor();
        MediaExtractor audioExtractor = new MediaExtractor();
        MediaCodec decoder = null;
        MediaCodec encoder = null;
        InputSurface encoderSurface = null;
        OutputSurface decoderSurface = null;
        MediaMuxer muxer = null;
        boolean decoderStarted = false;
        boolean encoderStarted = false;
        boolean muxerStarted = false;
        int audioTrackIndex = -1;
        try {
            videoExtractor.setDataSource(source.getAbsolutePath());
            int videoTrack = selectTrack(videoExtractor, true);
            if (videoTrack < 0) throw new IOException("Video track missing");
            MediaFormat inputFormat = videoExtractor.getTrackFormat(videoTrack);
            String inputMime = inputFormat.getString(MediaFormat.KEY_MIME);
            if (inputMime == null) throw new IOException("Video MIME missing");
            int sourceWidth = inputFormat.getInteger(MediaFormat.KEY_WIDTH);
            int sourceHeight = inputFormat.getInteger(MediaFormat.KEY_HEIGHT);
            if (sourceWidth <= 0 || sourceHeight <= 0) throw new IOException("Invalid video size");

            audioExtractor.setDataSource(source.getAbsolutePath());
            int audioTrack = selectTrack(audioExtractor, false);
            MediaFormat audioFormat = audioTrack >= 0
                    ? audioExtractor.getTrackFormat(audioTrack) : null;
            boolean hasAudio = audioFormat != null
                    && "audio/mp4a-latm".equals(audioFormat.getString(MediaFormat.KEY_MIME));

            decoder = createHardwareCodec(inputMime, false);
            encoder = createHardwareCodec(profile.codec.mimeType, true);
            MediaFormat outputFormat = MediaFormat.createVideoFormat(
                    profile.codec.mimeType, profile.width, profile.height);
            outputFormat.setInteger(MediaFormat.KEY_COLOR_FORMAT,
                    MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface);
            outputFormat.setInteger(MediaFormat.KEY_BIT_RATE, profile.bitrateBps);
            outputFormat.setInteger(MediaFormat.KEY_FRAME_RATE, profile.frameRate);
            outputFormat.setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2);
            if (Build.VERSION.SDK_INT >= 21) {
                outputFormat.setInteger(MediaFormat.KEY_BITRATE_MODE,
                        MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_VBR);
            }

            encoder.configure(outputFormat, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE);
            encoderSurface = new InputSurface(encoder.createInputSurface());
            encoderSurface.makeCurrent();
            decoderSurface = new OutputSurface(profile.width, profile.height);
            decoder.configure(inputFormat, decoderSurface.surface, null, 0);

            muxer = new MediaMuxer(destination.getAbsolutePath(), MediaMuxer.OutputFormat.MUXER_OUTPUT_MPEG_4);
            if (hasAudio) audioTrackIndex = muxer.addTrack(audioFormat);
            videoExtractor.selectTrack(videoTrack);
            if (hasAudio) audioExtractor.selectTrack(audioTrack);
            encoder.start();
            encoderStarted = true;
            decoder.start();
            decoderStarted = true;

            boolean decoderInputDone = false;
            boolean decoderOutputDone = false;
            boolean encoderOutputDone = false;
            boolean[] muxerState = new boolean[] { false };
            int[] videoTrackIndex = new int[] { -1 };
            long lastVideoPtsUs = 0;
            MediaCodec.BufferInfo decoderInfo = new MediaCodec.BufferInfo();
            MediaCodec.BufferInfo encoderInfo = new MediaCodec.BufferInfo();
            ByteBuffer audioBuffer = ByteBuffer.allocateDirect(1 * 1024 * 1024)
                    .order(ByteOrder.nativeOrder());

            while (!encoderOutputDone) {
                if (!decoderInputDone) {
                    int inputIndex = decoder.dequeueInputBuffer(TIMEOUT_US);
                    if (inputIndex >= 0) {
                        ByteBuffer input = decoder.getInputBuffer(inputIndex);
                        if (input == null) throw new IOException("Decoder input unavailable");
                        input.clear();
                        int sampleSize = videoExtractor.readSampleData(input, 0);
                        long sampleTimeUs = videoExtractor.getSampleTime();
                        if (sampleSize < 0) {
                            decoder.queueInputBuffer(inputIndex, 0, 0, 0,
                                    MediaCodec.BUFFER_FLAG_END_OF_STREAM);
                            decoderInputDone = true;
                        } else {
                            decoder.queueInputBuffer(inputIndex, 0, sampleSize,
                                    Math.max(0, sampleTimeUs), videoExtractor.getSampleFlags());
                            videoExtractor.advance();
                        }
                    }
                }

                if (!decoderOutputDone) {
                    int outputIndex = decoder.dequeueOutputBuffer(decoderInfo, TIMEOUT_US);
                    if (outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED
                            || outputIndex == MediaCodec.INFO_TRY_AGAIN_LATER) {
                        // Decoder format is represented by the configured output surface.
                    } else if (outputIndex >= 0) {
                        boolean endOfStream = (decoderInfo.flags
                                & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
                        if (!endOfStream) {
                            decoder.releaseOutputBuffer(outputIndex, true);
                            decoderSurface.drawFrame(encoderSurface, decoderInfo.presentationTimeUs);
                            lastVideoPtsUs = Math.max(lastVideoPtsUs, decoderInfo.presentationTimeUs);
                        } else {
                            decoder.releaseOutputBuffer(outputIndex, false);
                        }
                        if (endOfStream) {
                            decoderOutputDone = true;
                            encoder.signalEndOfInputStream();
                        }
                    }
                }

                if (muxerStarted && hasAudio) {
                    drainAudio(audioExtractor, audioBuffer, audioTrackIndex, muxer, lastVideoPtsUs);
                }
                encoderOutputDone = drainEncoder(encoder, encoderInfo, muxer,
                        audioExtractor, audioBuffer, audioTrackIndex, hasAudio,
                        muxerState, videoTrackIndex);
                muxerStarted = muxerState[0];
            }
            if (muxerStarted && hasAudio) {
                drainAudio(audioExtractor, audioBuffer, audioTrackIndex, muxer, Long.MAX_VALUE);
            }
            long durationUs = Math.max(lastVideoPtsUs, inputFormat.containsKey(MediaFormat.KEY_DURATION)
                    ? inputFormat.getLong(MediaFormat.KEY_DURATION) : 0);
            return new TranscodedVideo(destination, profile, durationUs, hasAudio);
        } finally {
            if (decoderStarted) decoder.stop();
            if (encoderStarted) encoder.stop();
            if (decoder != null) decoder.release();
            if (encoder != null) encoder.release();
            if (decoderSurface != null) decoderSurface.release();
            if (encoderSurface != null) encoderSurface.release();
            if (muxer != null) {
                if (muxerStarted) muxer.stop();
                muxer.release();
            }
            videoExtractor.release();
            audioExtractor.release();
        }
    }

    private static boolean drainEncoder(MediaCodec encoder, MediaCodec.BufferInfo info,
            MediaMuxer muxer, MediaExtractor audioExtractor, ByteBuffer audioBuffer,
            int audioTrackIndex, boolean hasAudio, boolean[] muxerStarted,
            int[] videoTrackIndex) throws IOException {
        while (true) {
            int outputIndex = encoder.dequeueOutputBuffer(info, 0);
            if (outputIndex == MediaCodec.INFO_TRY_AGAIN_LATER) return false;
            if (outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) {
                if (muxerStarted[0]) throw new IOException("Encoder format changed twice");
                videoTrackIndex[0] = muxer.addTrack(encoder.getOutputFormat());
                muxer.start();
                muxerStarted[0] = true;
                continue;
            }
            if (outputIndex < 0) continue;
            ByteBuffer output = encoder.getOutputBuffer(outputIndex);
            if (output == null) throw new IOException("Encoder output unavailable");
            if ((info.flags & MediaCodec.BUFFER_FLAG_CODEC_CONFIG) == 0 && info.size > 0) {
                if (!muxerStarted[0]) throw new IOException("Muxer not started");
                if (hasAudio) drainAudio(audioExtractor, audioBuffer, audioTrackIndex, muxer,
                        info.presentationTimeUs);
                output.position(info.offset);
                output.limit(info.offset + info.size);
                muxer.writeSampleData(videoTrackIndex[0], output, info);
            }
            boolean endOfStream = (info.flags & MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0;
            encoder.releaseOutputBuffer(outputIndex, false);
            if (endOfStream) {
                // The actual holder is updated by the caller after this method returns.
                info.flags = Integer.MIN_VALUE;
                return true;
            }
        }
    }

    private static void drainAudio(MediaExtractor extractor, ByteBuffer buffer,
            int trackIndex, MediaMuxer muxer, long throughPtsUs) throws IOException {
        if (trackIndex < 0) return;
        while (true) {
            long sampleTimeUs = extractor.getSampleTime();
            if (sampleTimeUs < 0 || sampleTimeUs > throughPtsUs) return;
            int sampleSize = (int) extractor.getSampleSize();
            if (sampleSize <= 0 || sampleSize > buffer.capacity())
                throw new IOException("Audio sample exceeds buffer");
            buffer.clear();
            int read = extractor.readSampleData(buffer, 0);
            if (read != sampleSize) throw new IOException("Audio sample read failed");
            MediaCodec.BufferInfo info = new MediaCodec.BufferInfo();
            info.set(0, sampleSize, sampleTimeUs, extractor.getSampleFlags());
            buffer.position(0);
            buffer.limit(sampleSize);
            muxer.writeSampleData(trackIndex, buffer, info);
            extractor.advance();
        }
    }

    private static int selectTrack(MediaExtractor extractor, boolean video) {
        for (int index = 0; index < extractor.getTrackCount(); index++) {
            String mime = extractor.getTrackFormat(index).getString(MediaFormat.KEY_MIME);
            if (mime != null && mime.startsWith(video ? "video/" : "audio/")) return index;
        }
        return -1;
    }

    private static MediaCodec createHardwareCodec(String mimeType, boolean encoder)
            throws IOException {
        MediaCodecList codecList = new MediaCodecList(MediaCodecList.REGULAR_CODECS);
        for (MediaCodecInfo info : codecList.getCodecInfos()) {
            if (info.isEncoder() != encoder || isSoftwareCodec(info)) continue;
            for (String supported : info.getSupportedTypes()) {
                if (mimeType.equalsIgnoreCase(supported)) {
                    try {
                        return MediaCodec.createByCodecName(info.getName());
                    } catch (IOException error) {
                        // Try the next hardware implementation.
                    }
                }
            }
        }
        throw new IOException("Hardware codec unavailable: " + mimeType);
    }

    private static boolean isSoftwareCodec(MediaCodecInfo info) {
        if (Build.VERSION.SDK_INT >= 29 && !info.isHardwareAccelerated()) return true;
        String name = info.getName().toLowerCase();
        return name.startsWith("omx.google.") || name.startsWith("c2.android.")
                || name.startsWith("c2.google.") || name.contains("software");
    }

    private static final class InputSurface {
        private final Surface surface;
        private final EGLDisplay display;
        private final EGLContext context;
        private final EGLSurface eglSurface;

        InputSurface(Surface surface) throws IOException {
            if (surface == null) throw new IOException("Encoder surface unavailable");
            this.surface = surface;
            display = EGL14.eglGetDisplay(EGL14.EGL_DEFAULT_DISPLAY);
            if (display == EGL14.EGL_NO_DISPLAY) throw new IOException("EGL display unavailable");
            int[] version = new int[2];
            if (!EGL14.eglInitialize(display, version, 0, version, 1))
                throw new IOException("EGL initialization failed");
            int[] attributes = {
                    EGL14.EGL_RED_SIZE, 8, EGL14.EGL_GREEN_SIZE, 8,
                    EGL14.EGL_BLUE_SIZE, 8, EGL14.EGL_ALPHA_SIZE, 8,
                    EGL14.EGL_RENDERABLE_TYPE, EGL14.EGL_OPENGL_ES2_BIT,
                    EGL_RECORDABLE_ANDROID, 1, EGL14.EGL_NONE
            };
            EGLConfig[] configs = new EGLConfig[1];
            int[] count = new int[1];
            if (!EGL14.eglChooseConfig(display, attributes, 0, configs, 0, 1, count, 0)
                    || count[0] == 0)
                throw new IOException("EGL config unavailable");
            int[] contextAttributes = {
                    EGL14.EGL_CONTEXT_CLIENT_VERSION, 2, EGL14.EGL_NONE
            };
            context = EGL14.eglCreateContext(display, configs[0], EGL14.EGL_NO_CONTEXT,
                    contextAttributes, 0);
            eglSurface = EGL14.eglCreateWindowSurface(display, configs[0], surface,
                    new int[] { EGL14.EGL_NONE }, 0);
            if (context == EGL14.EGL_NO_CONTEXT || eglSurface == EGL14.EGL_NO_SURFACE)
                throw new IOException("EGL surface creation failed");
        }

        void makeCurrent() throws IOException {
            if (!EGL14.eglMakeCurrent(display, eglSurface, eglSurface, context))
                throw new IOException("EGL make-current failed");
        }

        void setPresentationTime(long presentationTimeUs) {
            EGLExt.eglPresentationTimeANDROID(display, eglSurface,
                    presentationTimeUs * 1_000L);
        }

        void swapBuffers() throws IOException {
            if (!EGL14.eglSwapBuffers(display, eglSurface))
                throw new IOException("EGL swap failed");
        }

        void release() {
            EGL14.eglMakeCurrent(display, EGL14.EGL_NO_SURFACE, EGL14.EGL_NO_SURFACE,
                    EGL14.EGL_NO_CONTEXT);
            EGL14.eglDestroySurface(display, eglSurface);
            EGL14.eglDestroyContext(display, context);
            EGL14.eglReleaseThread();
            EGL14.eglTerminate(display);
            surface.release();
        }
    }

    private static final class OutputSurface {
        private final SurfaceTexture texture;
        private final Surface surface;
        private final TextureRender renderer = new TextureRender();
        private final int width;
        private final int height;

        OutputSurface(int width, int height) throws IOException {
            this.width = width;
            this.height = height;
            int textureId = renderer.createTexture();
            texture = new SurfaceTexture(textureId);
            texture.setDefaultBufferSize(width, height);
            surface = new Surface(texture);
        }

        void drawFrame(InputSurface inputSurface, long presentationTimeUs) throws IOException {
            texture.updateTexImage();
            renderer.draw(texture, width, height);
            inputSurface.setPresentationTime(presentationTimeUs);
            inputSurface.swapBuffers();
        }

        void release() {
            surface.release();
            texture.release();
            renderer.release();
        }
    }

    private static final class TextureRender {
        private static final float[] VERTICES = {
                -1f, -1f, 1f, -1f, -1f, 1f, 1f, 1f
        };
        private static final float[] TEX_COORDS = {
                0f, 1f, 1f, 1f, 0f, 0f, 1f, 0f
        };
        private static final String VERTEX_SHADER =
                "uniform mat4 uTexMatrix;" +
                "attribute vec4 aPosition;" +
                "attribute vec4 aTextureCoord;" +
                "varying vec2 vTextureCoord;" +
                "void main() { gl_Position = aPosition; " +
                "vTextureCoord = (uTexMatrix * aTextureCoord).xy; }";
        private static final String FRAGMENT_SHADER =
                "#extension GL_OES_EGL_image_external : require\n" +
                "precision mediump float;" +
                "uniform samplerExternalOES sTexture;" +
                "varying vec2 vTextureCoord;" +
                "void main() { gl_FragColor = texture2D(sTexture, vTextureCoord); }";

        private final FloatBuffer vertices = floatBuffer(VERTICES);
        private final FloatBuffer textureCoords = floatBuffer(TEX_COORDS);
        private final float[] textureMatrix = new float[16];
        private int program;
        private int textureId;
        private int positionHandle;
        private int textureHandle;
        private int matrixHandle;

        int createTexture() throws IOException {
            int[] textures = new int[1];
            GLES20.glGenTextures(1, textures, 0);
            textureId = textures[0];
            GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, textureId);
            GLES20.glTexParameterf(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                    GLES20.GL_TEXTURE_MIN_FILTER, GLES20.GL_NEAREST);
            GLES20.glTexParameterf(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                    GLES20.GL_TEXTURE_MAG_FILTER, GLES20.GL_LINEAR);
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                    GLES20.GL_TEXTURE_WRAP_S, GLES20.GL_CLAMP_TO_EDGE);
            GLES20.glTexParameteri(GLES11Ext.GL_TEXTURE_EXTERNAL_OES,
                    GLES20.GL_TEXTURE_WRAP_T, GLES20.GL_CLAMP_TO_EDGE);
            program = createProgram(VERTEX_SHADER, FRAGMENT_SHADER);
            positionHandle = GLES20.glGetAttribLocation(program, "aPosition");
            textureHandle = GLES20.glGetAttribLocation(program, "aTextureCoord");
            matrixHandle = GLES20.glGetUniformLocation(program, "uTexMatrix");
            if (positionHandle < 0 || textureHandle < 0 || matrixHandle < 0)
                throw new IOException("Video shader unavailable");
            return textureId;
        }

        void draw(SurfaceTexture source, int width, int height) {
            source.getTransformMatrix(textureMatrix);
            GLES20.glViewport(0, 0, width, height);
            GLES20.glUseProgram(program);
            GLES20.glActiveTexture(GLES20.GL_TEXTURE0);
            GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, textureId);
            GLES20.glUniform1i(textureHandle, 0);
            GLES20.glUniformMatrix4fv(matrixHandle, 1, false, textureMatrix, 0);
            GLES20.glEnableVertexAttribArray(positionHandle);
            GLES20.glVertexAttribPointer(positionHandle, 2, GLES20.GL_FLOAT, false,
                    0, vertices);
            GLES20.glEnableVertexAttribArray(textureHandle);
            GLES20.glVertexAttribPointer(textureHandle, 2, GLES20.GL_FLOAT, false,
                    0, textureCoords);
            GLES20.glDrawArrays(GLES20.GL_TRIANGLE_STRIP, 0, 4);
            GLES20.glDisableVertexAttribArray(positionHandle);
            GLES20.glDisableVertexAttribArray(textureHandle);
            GLES20.glBindTexture(GLES11Ext.GL_TEXTURE_EXTERNAL_OES, 0);
        }

        void release() {
            if (program != 0) GLES20.glDeleteProgram(program);
            if (textureId != 0) GLES20.glDeleteTextures(1, new int[] { textureId }, 0);
        }

        private static FloatBuffer floatBuffer(float[] values) {
            FloatBuffer buffer = ByteBuffer.allocateDirect(values.length * 4)
                    .order(ByteOrder.nativeOrder()).asFloatBuffer();
            buffer.put(values).position(0);
            return buffer;
        }

        private static int createProgram(String vertexSource, String fragmentSource)
                throws IOException {
            int vertex = compileShader(GLES20.GL_VERTEX_SHADER, vertexSource);
            int fragment = compileShader(GLES20.GL_FRAGMENT_SHADER, fragmentSource);
            int program = GLES20.glCreateProgram();
            GLES20.glAttachShader(program, vertex);
            GLES20.glAttachShader(program, fragment);
            GLES20.glLinkProgram(program);
            int[] linked = new int[1];
            GLES20.glGetProgramiv(program, GLES20.GL_LINK_STATUS, linked, 0);
            GLES20.glDeleteShader(vertex);
            GLES20.glDeleteShader(fragment);
            if (linked[0] == 0) {
                GLES20.glDeleteProgram(program);
                throw new IOException("Video shader link failed");
            }
            return program;
        }

        private static int compileShader(int type, String source) throws IOException {
            int shader = GLES20.glCreateShader(type);
            GLES20.glShaderSource(shader, source);
            GLES20.glCompileShader(shader);
            int[] compiled = new int[1];
            GLES20.glGetShaderiv(shader, GLES20.GL_COMPILE_STATUS, compiled, 0);
            if (compiled[0] == 0) {
                GLES20.glDeleteShader(shader);
                throw new IOException("Video shader compile failed");
            }
            return shader;
        }
    }
}
