package ai.links.app;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import android.graphics.Matrix;
import android.media.ExifInterface;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;

/** Decode, orient, proportionally downscale, and re-encode images before encryption. */
public final class AndroidImageResizer {
    public static final int MAX_IMAGE_EDGE = 1_600;
    public static final int MAX_INPUT_BYTES = 32 * 1024 * 1024;

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
        if (encodedImage == null || encodedImage.length == 0
                || encodedImage.length > MAX_INPUT_BYTES)
            throw new IOException("Invalid image input");

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
            Format format = outputFormat(bounds.outMimeType);
            ByteArrayOutputStream output = new ByteArrayOutputStream();
            if (!resized.compress(format.compressFormat, format.quality, output))
                throw new IOException("Unable to encode resized image");
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

    private static Format outputFormat(String inputMime) {
        if ("image/jpeg".equalsIgnoreCase(inputMime))
            return new Format(Bitmap.CompressFormat.JPEG, "image/jpeg", 95);
        if ("image/webp".equalsIgnoreCase(inputMime))
            return new Format(Bitmap.CompressFormat.WEBP, "image/webp", 95);
        return new Format(Bitmap.CompressFormat.PNG, "image/png", 100);
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
