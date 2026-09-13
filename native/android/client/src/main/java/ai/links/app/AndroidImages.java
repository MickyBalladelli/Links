package ai.links.app;

import android.content.Context;
import android.graphics.Bitmap;
import android.graphics.BitmapFactory;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.UUID;

/**
 * Android encrypted-image pipeline. The cache stores ciphertext only; decoded
 * pixels exist only long enough for rendering.
 */
public final class AndroidImages {
    public static final int MAX_PLAINTEXT_BYTES = 32 * 1024 * 1024;
    public static final int MAX_CIPHERTEXT_BYTES = MAX_PLAINTEXT_BYTES + 16;
    private static final String BASE83 =
            "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~";

    public static final class Metadata {
        public final String attachmentId;
        public final String mimeType;
        public final long ciphertextSizeBytes;
        public final byte[] contentKey;
        public final byte[] nonce;
        public final byte[] ciphertextSha256;
        public final int width;
        public final int height;
        public final String blurHash;

        public Metadata(String attachmentId, String mimeType, long ciphertextSizeBytes,
                byte[] contentKey, byte[] nonce, byte[] ciphertextSha256,
                int width, int height, String blurHash) throws IOException {
            requireUuid(attachmentId, "attachment ID");
            if (!isImageMime(mimeType) || ciphertextSizeBytes < 17
                    || ciphertextSizeBytes > MAX_CIPHERTEXT_BYTES
                    || contentKey == null || contentKey.length != 32
                    || nonce == null || nonce.length != 12
                    || ciphertextSha256 == null || ciphertextSha256.length != 32
                    || width <= 0 || width > AndroidImageResizer.MAX_IMAGE_EDGE
                    || height <= 0 || height > AndroidImageResizer.MAX_IMAGE_EDGE
                    || !isBlurHash(blurHash))
                throw new IOException("Invalid image metadata");
            this.attachmentId = attachmentId;
            this.mimeType = mimeType;
            this.ciphertextSizeBytes = ciphertextSizeBytes;
            this.contentKey = contentKey.clone();
            this.nonce = nonce.clone();
            this.ciphertextSha256 = ciphertextSha256.clone();
            this.width = width;
            this.height = height;
            this.blurHash = blurHash;
        }

        private static boolean isImageMime(String mimeType) {
            return "image/webp".equals(mimeType) || "image/avif".equals(mimeType);
        }
    }

    public static final class EncryptedImage {
        public final Metadata metadata;
        public final byte[] ciphertext;

        public EncryptedImage(Metadata metadata, byte[] ciphertext) throws IOException {
            if (metadata == null || ciphertext == null
                    || ciphertext.length != metadata.ciphertextSizeBytes
                    || !ciphertextMatches(metadata, ciphertext))
                throw new IOException("Invalid encrypted image");
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
            if (ciphertextSizeBytes < 17 || ciphertextSizeBytes > MAX_CIPHERTEXT_BYTES
                    || ciphertextSha256 == null || ciphertextSha256.length != 32)
                throw new IOException("Invalid image upload receipt");
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

    public interface Uploader {
        UploadReceipt upload(String accessToken, EncryptedImage image) throws Exception;
        byte[] download(String accessToken, Metadata metadata) throws Exception;
    }

    public interface Renderer {
        /** Renderer owns the Bitmap after this callback returns. */
        void render(Bitmap bitmap, String blurHash) throws Exception;
    }

    /** Encrypted-on-disk cache keyed only by canonical attachment UUID. */
    public static final class Cache {
        private final File directory;

        public Cache(Context context) throws IOException {
            if (context == null) throw new IOException("Missing image cache context");
            directory = new File(context.getApplicationContext().getCacheDir(),
                    "links-images-v1");
            if (!directory.exists() && !directory.mkdirs())
                throw new IOException("Unable to create image cache");
        }

        public synchronized void put(EncryptedImage image) throws IOException {
            if (image == null) throw new IOException("Missing image");
            File destination = fileFor(image.metadata.attachmentId);
            File temporary = File.createTempFile("image-", ".tmp", directory);
            try {
                try (FileOutputStream output = new FileOutputStream(temporary)) {
                    output.write(image.ciphertext);
                    output.flush();
                    output.getFD().sync();
                }
                if (!temporary.renameTo(destination) && !destination.exists())
                    throw new IOException("Unable to commit image cache entry");
            } finally {
                deleteQuietly(temporary);
            }
        }

        public synchronized byte[] get(Metadata metadata) throws IOException {
            if (metadata == null) throw new IOException("Missing image metadata");
            File file = fileFor(metadata.attachmentId);
            if (!file.isFile()) return null;
            byte[] ciphertext = Files.readAllBytes(file.toPath());
            if (!ciphertextMatches(metadata, ciphertext)) {
                deleteQuietly(file);
                return null;
            }
            return ciphertext;
        }

        public synchronized void remove(String attachmentId) throws IOException {
            requireUuid(attachmentId, "attachment ID");
            deleteQuietly(fileFor(attachmentId));
        }

        private File fileFor(String attachmentId) throws IOException {
            requireUuid(attachmentId, "attachment ID");
            return new File(directory, attachmentId + ".blob");
        }
    }

    private final ClientSession session;
    private final AndroidTextMessaging messaging;
    private final Uploader uploader;
    private final Cache cache;

    public AndroidImages(Context context, ClientSession session,
            AndroidTextMessaging messaging, Uploader uploader) throws IOException {
        if (session == null || messaging == null || uploader == null)
            throw new IOException("Invalid image session");
        this.session = session;
        this.messaging = messaging;
        this.uploader = uploader;
        this.cache = new Cache(context);
    }

    /** Resize, generate the shared BlurHash, and encrypt before upload. */
    public EncryptedImage prepareAndEncrypt(byte[] sourceImage) throws Exception {
        AndroidImageResizer.Result resized = AndroidImageResizer.resize(sourceImage);
        AndroidImageResizer.RgbPixels pixels =
                AndroidImageResizer.decodeRgb(resized.encoded);
        try {
            String blurHash = messaging.encodeImageBlurHash(
                    pixels.rgb, pixels.width, pixels.height);
            return messaging.encryptImage(
                    resized.encoded,
                    UUID.randomUUID().toString().toLowerCase(),
                    resized.mimeType,
                    resized.width,
                    resized.height,
                    blurHash);
        } finally {
            Arrays.fill(pixels.rgb, (byte) 0);
        }
    }

    public UploadReceipt upload(EncryptedImage image) throws Exception {
        if (image == null) throw new IOException("Missing image");
        UploadReceipt receipt = uploader.upload(session.accessToken(), image);
        if (receipt == null || !receipt.matches(image.metadata))
            throw new IOException("Image upload receipt mismatch");
        return receipt;
    }

    public void send(String conversationId, String recipientUserId,
            EncryptedImage image, UploadReceipt receipt) throws Exception {
        if (image == null || receipt == null || !receipt.matches(image.metadata))
            throw new IOException("Invalid image upload");
        messaging.sendImage(conversationId, recipientUserId, image.metadata, receipt);
    }

    /** Fetch ciphertext, decrypt it, then render verified pixels. */
    public void downloadAndRender(Metadata metadata, Renderer renderer) throws Exception {
        if (metadata == null || renderer == null)
            throw new IOException("Invalid image render request");
        byte[] ciphertext = cache.get(metadata);
        if (ciphertext == null) {
            ciphertext = uploader.download(session.accessToken(), metadata);
            EncryptedImage fetched = new EncryptedImage(metadata, ciphertext);
            cache.put(fetched);
        }

        byte[] plaintext = null;
        try {
            plaintext = messaging.decryptImage(metadata, ciphertext);
            Bitmap bitmap = decodeForRender(plaintext, metadata);
            renderer.render(bitmap, metadata.blurHash);
        } finally {
            Arrays.fill(ciphertext, (byte) 0);
            if (plaintext != null) Arrays.fill(plaintext, (byte) 0);
        }
    }

    private static Bitmap decodeForRender(byte[] plaintext, Metadata metadata)
            throws IOException {
        if (plaintext == null || plaintext.length == 0 || plaintext.length > MAX_PLAINTEXT_BYTES)
            throw new IOException("Invalid decrypted image");
        BitmapFactory.Options bounds = new BitmapFactory.Options();
        bounds.inJustDecodeBounds = true;
        BitmapFactory.decodeByteArray(plaintext, 0, plaintext.length, bounds);
        if (bounds.outWidth != metadata.width || bounds.outHeight != metadata.height)
            throw new IOException("Image dimensions do not match metadata");
        BitmapFactory.Options options = new BitmapFactory.Options();
        options.inScaled = false;
        options.inPreferredConfig = Bitmap.Config.ARGB_8888;
        Bitmap bitmap = BitmapFactory.decodeByteArray(plaintext, 0, plaintext.length, options);
        if (bitmap == null) throw new IOException("Unable to render image");
        return bitmap;
    }

    private static boolean ciphertextMatches(Metadata metadata, byte[] ciphertext) {
        return ciphertext.length == metadata.ciphertextSizeBytes
                && MessageDigest.isEqual(metadata.ciphertextSha256, sha256(ciphertext));
    }

    private static byte[] sha256(byte[] bytes) {
        try {
            return MessageDigest.getInstance("SHA-256").digest(bytes);
        } catch (Exception error) {
            throw new IllegalStateException("SHA-256 unavailable", error);
        }
    }

    private static boolean isBlurHash(String value) {
        if (value == null || value.length() != 28 || value.charAt(0) != 'L') return false;
        for (int index = 0; index < value.length(); index++) {
            if (BASE83.indexOf(value.charAt(index)) < 0) return false;
        }
        return true;
    }

    private static void requireUuid(String value, String field) throws IOException {
        try {
            UUID uuid = UUID.fromString(value);
            if (uuid.equals(new UUID(0, 0)) || !uuid.toString().equals(value))
                throw new IllegalArgumentException();
        } catch (IllegalArgumentException | NullPointerException error) {
            throw new IOException("Invalid " + field, error);
        }
    }

    private static void deleteQuietly(File file) {
        if (file != null && file.exists()) file.delete();
    }
}
