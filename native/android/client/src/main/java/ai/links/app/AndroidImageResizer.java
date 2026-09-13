package ai.links.app;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Matrix;
import android.media.ExifInterface;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;

/** Decode, orient, proportionally downscale, and transcode images before encryption. */
public final class AndroidImageResizer {
    public static final int MAX_IMAGE_EDGE = 1_600;
    public static final int MAX_INPUT_BYTES = 32 * 1024 * 1024;

    public static final class RgbPixels {
        public final byte[] rgb;
        public final int width;
        public final int height;

        private RgbPixels(byte[] rgb, int width, int height) {
            this.rgb = rgb;
            this.width = width;
            this.height = height;
        }
    }

    public static final class Result {
        public final byte[] encoded;
        public final String mimeType;
        public final int width;
        public final int height;

        private Result(byte[] encoded, String mimeType, int width, int height) {
            this.encoded = encoded;
            this.mimeType = mimeType;
            this.width = width;
            this.height = height;
        }
    }

    private AndroidImageResizer() {}

    public static Result resize(byte[] encodedImage) throws IOException {
        validateInput(encodedImage);

        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(encodedImage, 0, encodedImage.length, bounds);
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0 || bounds.outMimeType == null)
            throw new IOException("Unable to read image dimensions");

        int orientation = readOrientation(encodedImage);
        boolean swapsDimensions = swapsDimensions(orientation);
        int orientedWidth = swapsDimensions ? bounds.outHeight : bounds.outWidth;
        int orientedHeight = swapsDimensions ? bounds.outWidth : bounds.outHeight;
        Dimensions target = targetDimensions(orientedWidth, orientedHeight);
        int decodeTargetWidth = swapsDimensions ? target.height : target.width;
        int decodeTargetHeight = swapsDimensions ? target.width : target.height;
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight,
                decodeTargetWidth, decodeTargetHeight);
        options.inScaled = false;
        options.inPreferredConfig = Bitmap.Config.ARGB_8888;
        Bitmap decoded = BitmapFactory.decodeByteArray(encodedImage, 0, encodedImage.length,
                options);
        if (decoded == null) throw new IOException("Unable to decode image");

        Bitmap oriented = applyOrientation(decoded, orientation);
        Bitmap resized = oriented;
        try {
            if (oriented.getWidth() != target.width || oriented.getHeight() != target.height) {
                resized = Bitmap.createScaledBitmap(oriented, target.width, target.height, true);
            }
            Format format = outputFormat();
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            if (!resized.compress(format.compressFormat, format.quality, output))
                throw new IOException("Unable to encode WebP image");
            byte[] result = output.toByteArray();
            if (result.length == 0 || result.length > MAX_INPUT_BYTES)
                throw new IOException("Resized image exceeds size limit");
            return new Result(result, format.mimeType, resized.getWidth(), resized.getHeight());
        } finally {
            if (resized != oriented) resized.recycle();
            if (oriented != decoded) oriented.recycle();
            decoded.recycle();
        }
    }

    /** Decode normalized image pixels for shared-core BlurHash generation. */
    public static RgbPixels decodeRgb(byte[] encodedImage) throws IOException {
        validateInput(encodedImage);
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(encodedImage, 0, encodedImage.length, bounds);
        if (bounds.outWidth <= 0 || bounds.outHeight <= 0
                || bounds.outWidth > MAX_IMAGE_EDGE || bounds.outHeight > MAX_IMAGE_EDGE)
            throw new IOException("Invalid normalized image dimensions");

        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inScaled = false;
        options.inPreferredConfig = Bitmap.Config.ARGB_8888;
        Bitmap decoded = BitmapFactory.decodeByteArray(encodedImage, 0, encodedImage.length,
                options);
        if (decoded == null) throw new IOException("Unable to decode normalized image");
        long pixelCount = (long) decoded.getWidth() * decoded.getHeight();
        if (pixelCount <= 0 || pixelCount > (long) MAX_IMAGE_EDGE * MAX_IMAGE_EDGE
                || pixelCount > Integer.MAX_VALUE / 3) {
            decoded.recycle();
            throw new IOException("Normalized image is too large");
        }

        int[] argb = new int[(int) pixelCount];
        byte[] rgb = new byte[(int) pixelCount * 3];
        try {
            decoded.getPixels(argb, 0, decoded.getWidth(), 0, 0,
                    decoded.getWidth(), decoded.getHeight());
            for (int index = 0; index < argb.length; index++) {
                int color = argb[index];
                int offset = index * 3;
                rgb[offset] = (byte) ((color >> 16) & 0xff);
                rgb[offset + 1] = (byte) ((color >> 8) & 0xff);
                rgb[offset + 2] = (byte) (color & 0xff);
            }
            return new RgbPixels(rgb, decoded.getWidth(), decoded.getHeight());
        } finally {
            java.util.Arrays.fill(argb, 0);
            decoded.recycle();
        }
    }

    private static void validateInput(byte[] encodedImage) throws IOException {
        if (encodedImage == null || encodedImage.length == 0
                || encodedImage.length > MAX_INPUT_BYTES)
            throw new IOException("Invalid image input");
    }

    /** Re-encoding removes input EXIF, including GPS location tags. */
    private static int readOrientation(byte[] encodedImage) {
        try {
            ExifInterface exif = new ExifInterface(new ByteArrayInputStream(encodedImage));
            return exif.getAttributeInt(
                    ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL);
        } catch (IOException | IllegalArgumentException ignored) {
            return ExifInterface.ORIENTATION_NORMAL;
        }
    }

    private static boolean swapsDimensions(int orientation) {
        return orientation == ExifInterface.ORIENTATION_TRANSPOSE
                || orientation == ExifInterface.ORIENTATION_ROTATE_90
                || orientation == ExifInterface.ORIENTATION_TRANSVERSE
                || orientation == ExifInterface.ORIENTATION_ROTATE_270;
    }

    private static Bitmap applyOrientation(Bitmap bitmap, int orientation) {
        Matrix matrix = new Matrix();
        switch (orientation) {
            case ExifInterface.ORIENTATION_FLIP_HORIZONTAL:
                matrix.setScale(-1, 1);
                break;
            case ExifInterface.ORIENTATION_ROTATE_180:
                matrix.setRotate(180);
                break;
            case ExifInterface.ORIENTATION_FLIP_VERTICAL:
                matrix.setRotate(180);
                matrix.postScale(-1, 1);
                break;
            case ExifInterface.ORIENTATION_TRANSPOSE:
                matrix.setRotate(90);
                matrix.postScale(-1, 1);
                break;
            case ExifInterface.ORIENTATION_ROTATE_90:
                matrix.setRotate(90);
                break;
            case ExifInterface.ORIENTATION_TRANSVERSE:
                matrix.setRotate(-90);
                matrix.postScale(-1, 1);
                break;
            case ExifInterface.ORIENTATION_ROTATE_270:
                matrix.setRotate(-90);
                break;
            default:
                return bitmap;
        }
        return Bitmap.createBitmap(bitmap, 0, 0, bitmap.getWidth(), bitmap.getHeight(), matrix, true);
    }

    private static Dimensions targetDimensions(int width, int height) {
        if (width <= MAX_IMAGE_EDGE && height <= MAX_IMAGE_EDGE)
            return new Dimensions(width, height);
        if (width >= height)
            return new Dimensions(MAX_IMAGE_EDGE, scaled(height, width));
        return new Dimensions(scaled(width, height), MAX_IMAGE_EDGE);
    }

    private static int scaled(int edge, int longest) {
        long value = ((long) edge * MAX_IMAGE_EDGE + longest / 2L) / longest;
        return (int) Math.max(1, value);
    }

    private static int sampleSize(int width, int height, int targetWidth, int targetHeight) {
        int sample = 1;
        while (width / (sample * 2) >= targetWidth
                && height / (sample * 2) >= targetHeight
                && sample <= (1 << 29)) {
            sample *= 2;
        }
        return sample;
    }

    private static Format outputFormat() {
        return new Format(Bitmap.CompressFormat.WEBP, "image/webp", 80);
    }

    private static final class Dimensions {
        final int width;
        final int height;

        Dimensions(int width, int height) {
            this.width = width;
            this.height = height;
        }
    }

    private static final class Format {
        final Bitmap.CompressFormat compressFormat;
        final String mimeType;
        final int quality;

        Format(Bitmap.CompressFormat compressFormat, String mimeType, int quality) {
            this.compressFormat = compressFormat;
            this.mimeType = mimeType;
            this.quality = quality;
        }
    }
}
