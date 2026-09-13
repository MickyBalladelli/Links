package ai.links.app;

import android.Manifest;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.MediaPlayer;
import android.media.MediaRecorder;
import android.os.Build;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.Arrays;
import java.util.UUID;

/**
 * Android voice-note pipeline. Recording is local, upload receives only
 * ciphertext, and playback is opened only after the shared core verifies and
 * decrypts the downloaded attachment.
 */
public final class AndroidVoiceNotes {
    public static final int MIN_BITRATE_KBPS = 16;
    public static final int MAX_BITRATE_KBPS = 24;
    public static final int SAMPLE_RATE_HZ = 48_000;
    public static final int CHANNELS = 1;
    public static final int FRAME_DURATION_MS = 20;
    public static final int MAX_PLAINTEXT_BYTES = 16 * 1024 * 1024;
    public static final int MAX_CIPHERTEXT_BYTES = MAX_PLAINTEXT_BYTES + 16;

    public enum Container {
        OGG("audio/ogg"),
        OPUS("audio/ogg; codecs=opus");

        private final String mimeType;

        Container(String mimeType) {
            this.mimeType = mimeType;
        }

        public String mimeType() {
            return mimeType;
        }
    }

    public static final class Profile {
        public final Container container;
        public final int bitrateKbps;
        public final int sampleRateHz;
        public final int channels;
        public final int frameDurationMs;

        public Profile(Container container, int bitrateKbps, int sampleRateHz, int channels,
                int frameDurationMs) throws IOException {
            if (container == null || bitrateKbps < MIN_BITRATE_KBPS
                    || bitrateKbps > MAX_BITRATE_KBPS
                    || sampleRateHz != SAMPLE_RATE_HZ || channels != CHANNELS
                    || frameDurationMs != FRAME_DURATION_MS)
                throw new IOException("Invalid Opus voice profile");
            this.container = container;
            this.bitrateKbps = bitrateKbps;
            this.sampleRateHz = sampleRateHz;
            this.channels = channels;
            this.frameDurationMs = frameDurationMs;
        }

        public static Profile standard() throws IOException {
            return new Profile(Container.OGG, 24, SAMPLE_RATE_HZ, CHANNELS,
                    FRAME_DURATION_MS);
        }
    }

    public static final class Metadata {
        public final String attachmentId;
        public final String mimeType;
        public final long ciphertextSizeBytes;
        public final byte[] contentKey;
        public final byte[] nonce;
        public final byte[] ciphertextSha256;
        public final long durationMs;
        public final Profile profile;

        public Metadata(String attachmentId, String mimeType, long ciphertextSizeBytes,
                byte[] contentKey, byte[] nonce, byte[] ciphertextSha256, long durationMs,
                Profile profile) throws IOException {
            requireUuid(attachmentId, "attachment ID");
            if (mimeType == null || !mimeType.equals(profileMime(profile))
                    || ciphertextSizeBytes < 16 || ciphertextSizeBytes > MAX_CIPHERTEXT_BYTES
                    || contentKey == null || contentKey.length != 32
                    || nonce == null || nonce.length != 12
                    || ciphertextSha256 == null || ciphertextSha256.length != 32
                    || durationMs == 0 || profile == null)
                throw new IOException("Invalid voice metadata");
            this.attachmentId = attachmentId;
            this.mimeType = mimeType;
            this.ciphertextSizeBytes = ciphertextSizeBytes;
            this.contentKey = contentKey.clone();
            this.nonce = nonce.clone();
            this.ciphertextSha256 = ciphertextSha256.clone();
            this.durationMs = durationMs;
            this.profile = profile;
        }

        private static String profileMime(Profile profile) throws IOException {
            if (profile == null) throw new IOException("Missing voice profile");
            return profile.container.mimeType();
        }
    }

    public static final class EncryptedVoiceNote {
        public final Metadata metadata;
        public final byte[] ciphertext;

        public EncryptedVoiceNote(Metadata metadata, byte[] ciphertext) throws IOException {
            if (metadata == null || ciphertext == null
                    || ciphertext.length != metadata.ciphertextSizeBytes
                    || ciphertext.length > MAX_CIPHERTEXT_BYTES
                    || !MessageDigest.isEqual(metadata.ciphertextSha256, sha256(ciphertext)))
                throw new IOException("Invalid encrypted voice note");
            this.metadata = metadata;
            this.ciphertext = ciphertext.clone();
        }
    }

    public static final class UploadReceipt {
        public final String attachmentId;
        public final long ciphertextSizeBytes;
        public final byte[] ciphertextSha256;

        public UploadReceipt(String attachmentId, long ciphertextSizeBytes,
                byte[] ciphertextSha256) throws IOException {
            requireUuid(attachmentId, "attachment ID");
            if (ciphertextSizeBytes < 16 || ciphertextSizeBytes > MAX_CIPHERTEXT_BYTES
                    || ciphertextSha256 == null || ciphertextSha256.length != 32)
                throw new IOException("Invalid voice upload receipt");
            this.attachmentId = attachmentId;
            this.ciphertextSizeBytes = ciphertextSizeBytes;
            this.ciphertextSha256 = ciphertextSha256.clone();
        }

        public boolean matches(Metadata metadata) {
            return metadata != null && attachmentId.equals(metadata.attachmentId)
                    && ciphertextSizeBytes == metadata.ciphertextSizeBytes
                    && MessageDigest.isEqual(ciphertextSha256, metadata.ciphertextSha256);
        }
    }

    /** The implementation must use an authenticated TLS upload/download API. */
    public interface Uploader {
        UploadReceipt upload(String accessToken, EncryptedVoiceNote note) throws Exception;
        byte[] download(String accessToken, Metadata metadata) throws Exception;
    }

    private final Context context;
    private final ClientSession session;
    private final AndroidTextMessaging messaging;
    private final Uploader uploader;
    private final Profile profile;
    private final Object lock = new Object();
    private MediaRecorder recorder;
    private File recordingFile;
    private long recordingStartedAtMs;

    public AndroidVoiceNotes(Context context, ClientSession session,
            AndroidTextMessaging messaging, Uploader uploader, Profile profile) throws IOException {
        if (context == null || session == null || messaging == null || uploader == null
                || profile == null)
            throw new IOException("Invalid voice-note session");
        this.context = context.getApplicationContext();
        this.session = session;
        this.messaging = messaging;
        this.uploader = uploader;
        this.profile = profile;
    }

    public Profile profile() {
        return profile;
    }

    /** The host must request RECORD_AUDIO before calling this method. */
    public void startRecording() throws Exception {
        synchronized (lock) {
            if (recorder != null) throw new IOException("Voice recording already active");
            if (Build.VERSION.SDK_INT < 29)
                throw new IOException("Android Opus recording requires API 29");
            if (context.checkSelfPermission(Manifest.permission.RECORD_AUDIO)
                    != PackageManager.PERMISSION_GRANTED)
                throw new SecurityException("RECORD_AUDIO permission required");
            if (profile.container != Container.OGG)
                throw new IOException("Android recorder requires Ogg Opus");

            File file = File.createTempFile("links-voice-", ".ogg", context.getCacheDir());
            MediaRecorder created = new MediaRecorder();
            try {
                created.setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION);
                created.setOutputFormat(MediaRecorder.OutputFormat.OGG);
                created.setAudioEncoder(MediaRecorder.AudioEncoder.OPUS);
                created.setAudioEncodingBitRate(profile.bitrateKbps * 1_000);
                created.setAudioSamplingRate(profile.sampleRateHz);
                created.setAudioChannels(profile.channels);
                created.setOutputFile(file.getAbsolutePath());
                created.prepare();
                created.start();
            } catch (RuntimeException | IOException error) {
                created.release();
                deleteQuietly(file);
                throw new IOException("Unable to start voice recording", error);
            }
            recorder = created;
            recordingFile = file;
            recordingStartedAtMs = System.currentTimeMillis();
        }
    }

    /** Stop, validate, and encrypt the local Opus container. */
    public EncryptedVoiceNote stopRecordingAndEncrypt() throws Exception {
        MediaRecorder active;
        File file;
        long startedAt;
        synchronized (lock) {
            if (recorder == null || recordingFile == null)
                throw new IOException("Voice recording is not active");
            active = recorder;
            file = recordingFile;
            startedAt = recordingStartedAtMs;
            recorder = null;
            recordingFile = null;
        }

        long durationMs = Math.max(1, System.currentTimeMillis() - startedAt);
        try {
            active.stop();
        } catch (RuntimeException error) {
            throw new IOException("Unable to finish voice recording", error);
        } finally {
            active.release();
        }
        try {
            byte[] container = Files.readAllBytes(file.toPath());
            if (container.length == 0 || container.length > MAX_PLAINTEXT_BYTES)
                throw new IOException("Voice recording exceeds size limit");
            return messaging.encryptVoiceNote(container, UUID.randomUUID().toString(),
                    durationMs, profile);
        } finally {
            deleteQuietly(file);
        }
    }

    /** Upload ciphertext first. The returned receipt is needed for message send. */
    public UploadReceipt upload(EncryptedVoiceNote note) throws Exception {
        if (note == null) throw new IOException("Missing voice note");
        UploadReceipt receipt = uploader.upload(session.accessToken(), note);
        if (receipt == null || !receipt.matches(note.metadata))
            throw new IOException("Voice upload receipt mismatch");
        return receipt;
    }

    /** Send private metadata only after upload accepted the exact ciphertext. */
    public void send(String conversationId, String recipientUserId,
            EncryptedVoiceNote note, UploadReceipt receipt) throws Exception {
        if (note == null || receipt == null || !receipt.matches(note.metadata))
            throw new IOException("Invalid voice upload");
        messaging.sendVoiceNote(conversationId, recipientUserId, note.metadata, receipt);
    }

    /** Download ciphertext, decrypt it through shared core, then hand it to Android playback. */
    public PlaybackHandle downloadAndPlay(Metadata metadata) throws Exception {
        if (metadata == null) throw new IOException("Missing voice metadata");
        byte[] ciphertext = uploader.download(session.accessToken(), metadata);
        if (ciphertext == null || ciphertext.length != metadata.ciphertextSizeBytes
                || !MessageDigest.isEqual(metadata.ciphertextSha256, sha256(ciphertext)))
            throw new IOException("Voice download integrity failure");
        byte[] plaintext = messaging.decryptVoiceNote(metadata, ciphertext);
        if (plaintext == null || plaintext.length == 0 || plaintext.length > MAX_PLAINTEXT_BYTES)
            throw new IOException("Invalid decrypted voice note");

        File playbackFile = File.createTempFile("links-voice-playback-", ".ogg",
                context.getCacheDir());
        try (FileOutputStream output = new FileOutputStream(playbackFile)) {
            output.write(plaintext);
            output.flush();
        } finally {
            Arrays.fill(plaintext, (byte) 0);
        }

        MediaPlayer player = new MediaPlayer();
        PlaybackHandle handle = new PlaybackHandle(player, playbackFile);
        try {
            player.setDataSource(playbackFile.getAbsolutePath());
            player.setOnCompletionListener(view -> handle.close());
            player.setOnErrorListener((view, what, extra) -> {
                handle.close();
                return true;
            });
            player.prepare();
            player.start();
            return handle;
        } catch (Exception error) {
            handle.close();
            throw new IOException("Unable to play voice note", error);
        }
    }

    public void cancelRecording() {
        MediaRecorder active;
        File file;
        synchronized (lock) {
            active = recorder;
            file = recordingFile;
            recorder = null;
            recordingFile = null;
        }
        if (active != null) {
            try { active.stop(); } catch (RuntimeException ignored) {}
            active.release();
        }
        deleteQuietly(file);
    }

    public static final class PlaybackHandle implements AutoCloseable {
        private final MediaPlayer player;
        private final File file;
        private boolean closed;

        private PlaybackHandle(MediaPlayer player, File file) {
            this.player = player;
            this.file = file;
        }

        @Override
        public synchronized void close() {
            if (closed) return;
            closed = true;
            try { player.stop(); } catch (IllegalStateException ignored) {}
            player.release();
            deleteQuietly(file);
        }
    }

    private static byte[] sha256(byte[] bytes) throws IOException {
        try {
            return MessageDigest.getInstance("SHA-256").digest(bytes);
        } catch (NoSuchAlgorithmException error) {
            throw new IOException("SHA-256 unavailable", error);
        }
    }

    private static void requireUuid(String value, String name) throws IOException {
        try {
            UUID uuid = UUID.fromString(value);
            if (uuid.equals(new UUID(0, 0)) || !uuid.toString().equals(value))
                throw new IllegalArgumentException();
        } catch (IllegalArgumentException | NullPointerException error) {
            throw new IOException("Invalid " + name, error);
        }
    }

    private static void deleteQuietly(File file) {
        if (file != null && file.exists() && !file.delete()) file.deleteOnExit();
    }
}
