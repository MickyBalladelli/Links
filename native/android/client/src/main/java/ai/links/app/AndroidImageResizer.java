package ai.links.app;

import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import java.io.ByteArrayOutputStream;
import java.io.IOException;

/** Decode, proportionally downscale, and re-encode images before encryption. */
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

        if (bounds.outWidth <= MAX_IMAGE_EDGE && bounds.outHeight <= MAX_IMAGE_EDGE) {
            return new Result(encodedImage.clone(), bounds.outMimeType,
                    bounds.outWidth, bounds.outHeight);
        }

        Dimensions target = targetDimensions(bounds.outWidth, bounds.outHeight);
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inSampleSize = sampleSize(bounds.outWidth, bounds.outHeight,
                target.width, target.height);
        options.inScaled = false;
        options.inPreferredConfig = Bitmap.Config.ARGB_8888;
        Bitmap decoded = BitmapFactory.decodeByteArray(encodedImage, 0, encodedImage.length,
                options);
        if (decoded == null) throw new IOException("Unable to decode image");

        Bitmap resized = decoded;
        try {
            if (decoded.getWidth() != target.width || decoded.getHeight() != target.height) {
                resized = Bitmap.createScaledBitmap(decoded, target.width, target.height, true);
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
            if (resized != decoded) resized.recycle();
            decoded.recycle();
        }
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
