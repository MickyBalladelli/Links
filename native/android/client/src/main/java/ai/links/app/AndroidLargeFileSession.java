package ai.links.app;

import java.io.File;
import java.io.IOException;
import java.security.MessageDigest;
import java.util.UUID;

/** Android orchestration for encrypted video and arbitrary large files. */
public final class AndroidLargeFileSession {
    public interface Uploader {
        UploadReceipt upload(String accessToken, AndroidLargeFileTransfer.EncryptedLargeFile file)
                throws Exception;
        /** Return a disposable ciphertext staging file. */
        File download(String accessToken, AndroidLargeFileTransfer.Metadata metadata)
                throws Exception;
    }

    public static final class UploadReceipt {
        public final String attachmentId;
        public final long ciphertextSizeBytes;
        public final byte[] ciphertextSha256;

        public UploadReceipt(String attachmentId, long ciphertextSizeBytes,
                byte[] ciphertextSha256) throws IOException {
            if (!isCanonicalUuid(attachmentId) || ciphertextSizeBytes <= 0
                    || ciphertextSha256 == null || ciphertextSha256.length != 32)
                throw new IOException("Invalid large-file upload receipt");
            this.attachmentId = attachmentId;
            this.ciphertextSizeBytes = ciphertextSizeBytes;
            this.ciphertextSha256 = ciphertextSha256.clone();
        }

        public boolean matches(AndroidLargeFileTransfer.Metadata metadata) {
            return metadata != null && attachmentId.equals(metadata.attachmentId)
                    && ciphertextSizeBytes == metadata.ciphertextSizeBytes
                    && MessageDigest.isEqual(ciphertextSha256, metadata.ciphertextSha256);
        }
    }

    private final ClientSession session;
    private final AndroidTextMessaging messaging;
    private final Uploader uploader;
    private final AndroidLargeFileTransfer transfer;

    public AndroidLargeFileSession(ClientSession session, AndroidTextMessaging messaging,
            Uploader uploader, AndroidLargeFileTransfer transfer) throws IOException {
        if (session == null || messaging == null || uploader == null || transfer == null)
            throw new IOException("Invalid large-file session");
        this.session = session;
        this.messaging = messaging;
        this.uploader = uploader;
        this.transfer = transfer;
    }

    public AndroidLargeFileTransfer.EncryptedLargeFile encrypt(File source,
            File destinationDirectory, String attachmentId, String mimeType,
            Integer width, Integer height, Long durationMs) throws IOException {
        return transfer.encrypt(source, destinationDirectory, attachmentId, mimeType,
                width, height, durationMs);
    }

    public AndroidLargeFileTransfer.EncryptedLargeFile encryptVideo(File transcodedMp4,
            File destinationDirectory, int width, int height, long durationMs) throws IOException {
        return encrypt(transcodedMp4, destinationDirectory, UUID.randomUUID().toString(),
                "video/mp4", width, height, durationMs);
    }

    public AndroidLargeFileTransfer.EncryptedLargeFile encryptFile(File source,
            File destinationDirectory) throws IOException {
        return encrypt(source, destinationDirectory, UUID.randomUUID().toString(),
                "application/octet-stream", null, null, null);
    }

    public UploadReceipt upload(AndroidLargeFileTransfer.EncryptedLargeFile file)
            throws Exception {
        if (file == null || file.metadata == null || file.ciphertextFile == null
                || !file.ciphertextFile.isFile()
                || file.ciphertextFile.length() != file.metadata.ciphertextSizeBytes)
            throw new IOException("Invalid encrypted large file");
        UploadReceipt receipt = uploader.upload(session.accessToken(), file);
        if (receipt == null || !receipt.matches(file.metadata))
            throw new IOException("Large-file upload receipt mismatch");
        return receipt;
    }

    public void send(String conversationId, String recipientUserId,
            AndroidLargeFileTransfer.EncryptedLargeFile file, UploadReceipt receipt)
            throws Exception {
        if (file == null || receipt == null || !receipt.matches(file.metadata))
            throw new IOException("Invalid large-file upload");
        messaging.sendLargeFile(conversationId, recipientUserId, file.metadata, receipt);
    }

    public void downloadAndDecrypt(AndroidLargeFileTransfer.Metadata metadata,
            File destination) throws Exception {
        if (metadata == null || destination == null || destination.exists())
            throw new IOException("Invalid large-file destination");
        File ciphertext = uploader.download(session.accessToken(), metadata);
        if (ciphertext == null) throw new IOException("Missing large-file ciphertext");
        try {
            transfer.decrypt(ciphertext, destination, metadata);
        } finally {
            if (ciphertext.exists()) ciphertext.delete();
        }
    }

    private static boolean isCanonicalUuid(String value) {
        if (value == null) return false;
        try {
            UUID uuid = UUID.fromString(value);
            return !uuid.equals(new UUID(0, 0)) && uuid.toString().equals(value);
        } catch (IllegalArgumentException error) {
            return false;
        }
    }
}
