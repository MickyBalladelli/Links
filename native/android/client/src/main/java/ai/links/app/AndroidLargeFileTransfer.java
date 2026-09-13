package ai.links.app;

import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Arrays;
import java.util.UUID;
import javax.crypto.Cipher;
import javax.crypto.spec.IvParameterSpec;
import javax.crypto.spec.SecretKeySpec;

/** Streams chunked ChaCha20-Poly1305 encryption for MP4 and large files. */
public final class AndroidLargeFileTransfer {
    public static final int CIPHERTEXT_CHUNK_BYTES = 256 * 1024;
    public static final int PLAINTEXT_CHUNK_BYTES = CIPHERTEXT_CHUNK_BYTES - 16;
    private static final byte[] AAD_PREFIX =
            "links/large-file/attachment/v1\0".getBytes(StandardCharsets.UTF_8);

    private final SecureRandom secureRandom;

    public AndroidLargeFileTransfer() {
        secureRandom = new SecureRandom();
    }

    public static final class Metadata {
        public final String attachmentId;
        public final String mimeType;
        public final long originalSizeBytes;
        public final long ciphertextSizeBytes;
        public final byte[] contentKey;
        public final byte[] nonce;
        public final byte[] ciphertextSha256;
        public final Integer width;
        public final Integer height;
        public final Long durationMs;

        public Metadata(String attachmentId, String mimeType, long originalSizeBytes,
                long ciphertextSizeBytes, byte[] contentKey, byte[] nonce,
                byte[] ciphertextSha256, Integer width, Integer height, Long durationMs)
                throws IOException {
            if (!isCanonicalUuid(attachmentId)
                    || !("video/mp4".equals(mimeType)
                    || "application/octet-stream".equals(mimeType))
                    || originalSizeBytes <= 0 || ciphertextSizeBytes <= 0
                    || contentKey == null || contentKey.length != 32
                    || nonce == null || nonce.length != 12
                    || ciphertextSha256 == null || ciphertextSha256.length != 32
                    || (width != null && width <= 0)
                    || (height != null && height <= 0)
                    || (durationMs != null && durationMs <= 0)) {
                throw new IOException("Invalid large-file metadata");
            }
            boolean isVideo = "video/mp4".equals(mimeType);
            if (isVideo != (width != null && height != null && durationMs != null))
                throw new IOException("Invalid large-file media metadata");
            long expected = expectedCiphertextSize(originalSizeBytes);
            if (expected != ciphertextSizeBytes)
                throw new IOException("Invalid large-file ciphertext size");
            this.attachmentId = attachmentId;
            this.mimeType = mimeType;
            this.originalSizeBytes = originalSizeBytes;
            this.ciphertextSizeBytes = ciphertextSizeBytes;
            this.contentKey = contentKey.clone();
            this.nonce = nonce.clone();
            this.ciphertextSha256 = ciphertextSha256.clone();
            this.width = width;
            this.height = height;
            this.durationMs = durationMs;
        }

        private static boolean isCanonicalUuid(String value) {
            if (value == null) return false;
            try {
                return UUID.fromString(value).toString().equals(value);
            } catch (IllegalArgumentException error) {
                return false;
            }
        }
    }

    public static final class EncryptedLargeFile {
        public final Metadata metadata;
        public final File ciphertextFile;

        private EncryptedLargeFile(Metadata metadata, File ciphertextFile) {
            this.metadata = metadata;
            this.ciphertextFile = ciphertextFile;
        }
    }

    public EncryptedLargeFile encrypt(File source, File destinationDirectory,
            String attachmentId, String mimeType, Integer width, Integer height,
            Long durationMs) throws IOException {
        if (source == null || destinationDirectory == null || !source.isFile()
                || source.length() <= 0 || !destinationDirectory.isDirectory()
                || !isCanonicalUuid(attachmentId))
            throw new IOException("Invalid large-file source");
        byte[] key = new byte[32];
        byte[] nonce = new byte[12];
        secureRandom.nextBytes(key);
        secureRandom.nextBytes(nonce);
        File output = File.createTempFile(".links-encrypted-", ".blob", destinationDirectory);
        try {
            MessageDigest digest = sha256();
            long plaintextSize = 0;
            long ciphertextSize = 0;
            long chunkIndex = 0;
            try (InputStream input = new FileInputStream(source);
                    OutputStream stream = new FileOutputStream(output)) {
                byte[] buffer = new byte[PLAINTEXT_CHUNK_BYTES];
                while (true) {
                    int plaintextBytes = readChunk(input, buffer);
                    if (plaintextBytes == 0) break;
                    byte[] plaintext = Arrays.copyOf(buffer, plaintextBytes);
                    byte[] ciphertext = crypt(Cipher.ENCRYPT_MODE, plaintext, key, nonce,
                            attachmentId, chunkIndex);
                    stream.write(ciphertext);
                    digest.update(ciphertext);
                    plaintextSize = checkedAdd(plaintextSize, plaintextBytes);
                    ciphertextSize = checkedAdd(ciphertextSize, ciphertext.length);
                    chunkIndex = checkedAdd(chunkIndex, 1);
                }
                stream.flush();
            }
            if (plaintextSize != source.length())
                throw new IOException("Large-file source changed during encryption");
            Metadata metadata = new Metadata(attachmentId, mimeType, plaintextSize,
                    ciphertextSize, key, nonce, digest.digest(), width, height, durationMs);
            return new EncryptedLargeFile(metadata, output);
        } catch (Exception error) {
            if (output.exists()) output.delete();
            if (error instanceof IOException) throw (IOException) error;
            throw new IOException("Large-file encryption failed", error);
        }
    }

    public void decrypt(File source, File destination, Metadata metadata) throws IOException {
        if (source == null || destination == null || metadata == null || !source.isFile()
                || source.length() != metadata.ciphertextSizeBytes || destination.exists())
            throw new IOException("Invalid large-file decryption input");
        File parent = destination.getAbsoluteFile().getParentFile();
        if (parent == null || !parent.isDirectory())
            throw new IOException("Missing large-file destination directory");
        File staging = File.createTempFile(".links-decrypted-", ".tmp", parent);
        try {
            MessageDigest digest = sha256();
            long plaintextSize = 0;
            long ciphertextSize = 0;
            long chunkCount = chunkCount(metadata.originalSizeBytes);
            try (InputStream input = new FileInputStream(source);
                    OutputStream output = new FileOutputStream(staging)) {
                for (long chunkIndex = 0; chunkIndex < chunkCount; chunkIndex++) {
                    long chunkStart = checkedMultiply(chunkIndex, PLAINTEXT_CHUNK_BYTES);
                    long remaining = metadata.originalSizeBytes - chunkStart;
                    int plaintextBytes = (int) Math.min(remaining, PLAINTEXT_CHUNK_BYTES);
                    int ciphertextBytes = plaintextBytes + 16;
                    byte[] ciphertext = new byte[ciphertextBytes];
                    readFully(input, ciphertext);
                    digest.update(ciphertext);
                    byte[] plaintext = crypt(Cipher.DECRYPT_MODE, ciphertext,
                            metadata.contentKey, metadata.nonce, metadata.attachmentId, chunkIndex);
                    if (plaintext.length != plaintextBytes)
                        throw new IOException("Invalid large-file plaintext size");
                    output.write(plaintext);
                    plaintextSize = checkedAdd(plaintextSize, plaintext.length);
                    ciphertextSize = checkedAdd(ciphertextSize, ciphertext.length);
                }
                if (input.read() != -1)
                    throw new IOException("Trailing large-file ciphertext");
                output.flush();
            }
            if (plaintextSize != metadata.originalSizeBytes
                    || ciphertextSize != metadata.ciphertextSizeBytes
                    || !MessageDigest.isEqual(digest.digest(), metadata.ciphertextSha256))
                throw new IOException("Large-file integrity failure");
            if (!staging.renameTo(destination))
                throw new IOException("Cannot publish decrypted large file");
        } catch (Exception error) {
            if (staging.exists()) staging.delete();
            if (error instanceof IOException) throw (IOException) error;
            throw new IOException("Large-file decryption failed", error);
        }
    }

    private static byte[] crypt(int mode, byte[] input, byte[] key, byte[] nonce,
            String attachmentId, long chunkIndex) throws Exception {
        Cipher cipher = Cipher.getInstance("ChaCha20-Poly1305");
        cipher.init(mode, new SecretKeySpec(key, "ChaCha20"),
                new IvParameterSpec(chunkNonce(nonce, chunkIndex)));
        cipher.updateAAD(chunkAad(attachmentId, chunkIndex));
        return cipher.doFinal(input);
    }

    private static byte[] chunkNonce(byte[] base, long chunkIndex) {
        byte[] result = base.clone();
        byte[] index = ByteBuffer.allocate(Long.BYTES).putLong(chunkIndex).array();
        for (int offset = 0; offset < index.length; offset++)
            result[4 + offset] ^= index[offset];
        return result;
    }

    private static byte[] chunkAad(String attachmentId, long chunkIndex) throws IOException {
        ByteArrayOutputStream aad = new ByteArrayOutputStream(
                AAD_PREFIX.length + attachmentId.getBytes(StandardCharsets.UTF_8).length + 8);
        aad.write(AAD_PREFIX, 0, AAD_PREFIX.length);
        byte[] attachmentBytes = attachmentId.getBytes(StandardCharsets.UTF_8);
        aad.write(attachmentBytes, 0, attachmentBytes.length);
        byte[] index = ByteBuffer.allocate(Long.BYTES).putLong(chunkIndex).array();
        aad.write(index, 0, index.length);
        return aad.toByteArray();
    }

    private static boolean isCanonicalUuid(String value) {
        if (value == null) return false;
        try {
            return UUID.fromString(value).toString().equals(value);
        } catch (IllegalArgumentException error) {
            return false;
        }
    }

    private static long expectedCiphertextSize(long originalSize) throws IOException {
        long chunks = chunkCount(originalSize);
        return checkedAdd(originalSize, checkedMultiply(chunks, 16));
    }

    private static long chunkCount(long size) throws IOException {
        if (size <= 0) throw new IOException("Invalid large-file size");
        return checkedAdd(size, PLAINTEXT_CHUNK_BYTES - 1) / PLAINTEXT_CHUNK_BYTES;
    }

    private static int readChunk(InputStream input, byte[] buffer) throws IOException {
        int total = 0;
        while (total < buffer.length) {
            int count = input.read(buffer, total, buffer.length - total);
            if (count < 0) break;
            if (count == 0) continue;
            total += count;
        }
        return total;
    }

    private static void readFully(InputStream input, byte[] buffer) throws IOException {
        int total = 0;
        while (total < buffer.length) {
            int count = input.read(buffer, total, buffer.length - total);
            if (count < 0) throw new IOException("Truncated large-file ciphertext");
            if (count == 0) continue;
            total += count;
        }
    }

    private static MessageDigest sha256() throws IOException {
        try {
            return MessageDigest.getInstance("SHA-256");
        } catch (Exception error) {
            throw new IOException("SHA-256 unavailable", error);
        }
    }

    private static long checkedAdd(long left, long right) throws IOException {
        if (right < 0 || left > Long.MAX_VALUE - right)
            throw new IOException("Large-file size overflow");
        return left + right;
    }

    private static long checkedMultiply(long left, long right) throws IOException {
        if (left < 0 || right < 0 || (left != 0 && right > Long.MAX_VALUE / left))
            throw new IOException("Large-file size overflow");
        return left * right;
    }
}
