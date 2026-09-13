package ai.links.app;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/**
 * Android one-to-one text session. The injected bridge owns shared Rust
 * send/receive state, MLS persistence, protobuf framing and local rendering.
 * Voice notes are exposed through the separate AndroidVoiceNotes pipeline.
 */
public final class AndroidTextMessaging {
    public static final int MAX_TEXT_BYTES = 64 * 1024;

    public enum State { STOPPED, CONNECTING, READY, FAILED }

    public interface CoreBridge {
        long durableCursor() throws Exception;
        byte[] createHello(String deviceId, String accessToken, long durableCursor) throws Exception;
        /** Process one authenticated server frame and report only committed messages. */
        void handleServerFrame(byte[] frame, ConnectionManager connection, Listener listener)
                throws Exception;
        /** Call links-client-core::send::send_text and persist its exact outbox result. */
        void sendText(String conversationId, String recipientUserId, String text,
                ConnectionManager connection) throws Exception;
        /** Encode and encrypt voice bytes through the shared Rust client core. */
        default AndroidVoiceNotes.EncryptedVoiceNote encryptVoiceNote(byte[] opusContainer,
                String attachmentId, long durationMs, AndroidVoiceNotes.Profile profile)
                throws Exception {
            throw new IOException("Voice core unavailable");
        }
        /** Encode normalized RGB pixels with the shared BlurHash contract. */
        default String encodeImageBlurHash(byte[] rgbPixels, int width, int height)
                throws Exception {
            throw new IOException("Image core unavailable");
        }
        /** Encrypt transcoded image bytes; media metadata stays inside MLS. */
        default AndroidImages.EncryptedImage encryptImage(byte[] imageBytes,
                String attachmentId, String mimeType, int width, int height, String blurHash)
                throws Exception {
            throw new IOException("Image core unavailable");
        }
        /** Verify metadata, digest, AEAD and Opus framing before playback. */
        default byte[] decryptVoiceNote(AndroidVoiceNotes.Metadata metadata, byte[] ciphertext)
                throws Exception {
            throw new IOException("Voice core unavailable");
        }
        /** Verify image metadata and decrypt bytes before rendering. */
        default byte[] decryptImage(AndroidImages.Metadata metadata, byte[] ciphertext)
                throws Exception {
            throw new IOException("Image core unavailable");
        }
        /** Upload has already been accepted; now send private MediaMetadata in MLS. */
        default void sendVoiceNote(String conversationId, String recipientUserId,
                AndroidVoiceNotes.Metadata metadata, AndroidVoiceNotes.UploadReceipt receipt,
                ConnectionManager connection) throws Exception {
            throw new IOException("Voice core unavailable");
        }
        /** Send private image metadata only after its ciphertext upload receipt. */
        default void sendImage(String conversationId, String recipientUserId,
                AndroidImages.Metadata metadata, AndroidImages.UploadReceipt receipt,
                ConnectionManager connection) throws Exception {
            throw new IOException("Image core unavailable");
        }
        /** Send private video/file metadata after its ciphertext receipt. */
        default void sendLargeFile(String conversationId, String recipientUserId,
                AndroidLargeFileTransfer.Metadata metadata,
                AndroidLargeFileSession.UploadReceipt receipt,
                ConnectionManager connection) throws Exception {
            throw new IOException("Large-file core unavailable");
        }
    }

    public interface Listener {
        void onState(State state);
        void onTextMessage(String conversationId, String senderDeviceId, String text,
                long sequenceId, long sentAtMs);
        void onFailure();
    }

    private final Object lock = new Object();
    private final String endpoint;
    private final ClientSession session;
    private final CoreBridge bridge;
    private final Listener listener;
    private ConnectionManager connection;
    private State state = State.STOPPED;

    public AndroidTextMessaging(String endpoint, ClientSession session, CoreBridge bridge,
            Listener listener) throws IOException {
        if (endpoint == null || endpoint.isEmpty() || session == null || bridge == null
                || listener == null)
            throw new IOException("Invalid text session");
        this.endpoint = endpoint;
        this.session = session;
        this.bridge = bridge;
        this.listener = listener;
    }

    public State state() {
        synchronized (lock) { return state; }
    }

    public void start() throws Exception {
        synchronized (lock) {
            if (state == State.CONNECTING || state == State.READY) return;
            if (!session.isAuthenticated()) throw new IOException("Authenticated session required");
            state = State.CONNECTING;
        }
        listener.onState(State.CONNECTING);
        final ConnectionManager[] holder = new ConnectionManager[1];
        ConnectionManager created = new ConnectionManager(endpoint,
                () -> {
                    long cursor = bridge.durableCursor();
                    return bridge.createHello(session.deviceId(), session.accessToken(), cursor);
                },
                new ConnectionManager.Listener() {
                    @Override
                    public void onConnected() {
                        updateState(State.READY);
                    }

                    @Override
                    public void onBinaryFrame(byte[] frame) {
                        try {
                            bridge.handleServerFrame(frame, holder[0], listener);
                        } catch (Exception error) {
                            updateState(State.FAILED);
                            listener.onFailure();
                            holder[0].stop();
                        }
                    }

                    @Override
                    public void onDisconnected() {
                        boolean notify;
                        synchronized (lock) {
                            notify = state != State.STOPPED && state != State.FAILED;
                            if (notify) state = State.CONNECTING;
                        }
                        if (notify) listener.onState(State.CONNECTING);
                    }

                    @Override
                    public void onFailure() {
                        updateState(State.FAILED);
                        listener.onFailure();
                    }
                });
        holder[0] = created;
        synchronized (lock) {
            if (state != State.CONNECTING) {
                created.shutdown();
                return;
            }
            connection = created;
        }
        created.start();
    }

    /** Send one text message through the shared Rust direct-chat coordinator. */
    public void sendText(String conversationId, String recipientUserId, String text)
            throws Exception {
        requireUuid(conversationId, "conversation ID");
        requireUuid(recipientUserId, "recipient user ID");
        if (text == null || text.isEmpty()
                || text.getBytes(StandardCharsets.UTF_8).length > MAX_TEXT_BYTES)
            throw new IOException("Invalid text message");
        ConnectionManager active;
        synchronized (lock) {
            active = connection;
            if (state != State.READY || active == null || !active.isConnected())
                throw new IOException("Text session is not connected");
        }
        bridge.sendText(conversationId, recipientUserId, text, active);
    }

    /** Prepare one voice note through the same authenticated shared core instance. */
    public AndroidVoiceNotes.EncryptedVoiceNote encryptVoiceNote(byte[] opusContainer,
            String attachmentId, long durationMs, AndroidVoiceNotes.Profile profile)
            throws Exception {
        CoreBridge activeBridge;
        synchronized (lock) {
            activeBridge = bridge;
            if (state != State.READY || coreFailed())
                throw new IOException("Voice session is not ready");
        }
        return activeBridge.encryptVoiceNote(opusContainer, attachmentId, durationMs, profile);
    }

    /** Generate the shared low-resolution placeholder from normalized RGB pixels. */
    public String encodeImageBlurHash(byte[] rgbPixels, int width, int height) throws Exception {
        CoreBridge activeBridge;
        synchronized (lock) {
            activeBridge = bridge;
            if (state != State.READY || coreFailed())
                throw new IOException("Image session is not ready");
        }
        return activeBridge.encodeImageBlurHash(rgbPixels, width, height);
    }

    /** Encrypt normalized image bytes through the shared Rust core. */
    public AndroidImages.EncryptedImage encryptImage(byte[] imageBytes, String attachmentId,
            String mimeType, int width, int height, String blurHash) throws Exception {
        CoreBridge activeBridge;
        synchronized (lock) {
            activeBridge = bridge;
            if (state != State.READY || coreFailed())
                throw new IOException("Image session is not ready");
        }
        return activeBridge.encryptImage(
                imageBytes, attachmentId, mimeType, width, height, blurHash);
    }

    /** Decrypt one downloaded attachment through the shared core before playback. */
    public byte[] decryptVoiceNote(AndroidVoiceNotes.Metadata metadata, byte[] ciphertext)
            throws Exception {
        return bridge.decryptVoiceNote(metadata, ciphertext);
    }

    /** Decrypt verified image ciphertext through the shared core. */
    public byte[] decryptImage(AndroidImages.Metadata metadata, byte[] ciphertext)
            throws Exception {
        return bridge.decryptImage(metadata, ciphertext);
    }

    /** Send private media metadata after the opaque attachment upload succeeds. */
    public void sendVoiceNote(String conversationId, String recipientUserId,
            AndroidVoiceNotes.Metadata metadata, AndroidVoiceNotes.UploadReceipt receipt)
            throws Exception {
        requireUuid(conversationId, "conversation ID");
        requireUuid(recipientUserId, "recipient user ID");
        if (metadata == null || receipt == null || !receipt.matches(metadata))
            throw new IOException("Invalid voice upload receipt");
        ConnectionManager active;
        synchronized (lock) {
            active = connection;
            if (state != State.READY || active == null || !active.isConnected())
                throw new IOException("Voice session is not connected");
        }
        bridge.sendVoiceNote(conversationId, recipientUserId, metadata, receipt, active);
    }

    /** Send private image metadata only after the exact ciphertext upload receipt. */
    public void sendImage(String conversationId, String recipientUserId,
            AndroidImages.Metadata metadata, AndroidImages.UploadReceipt receipt)
            throws Exception {
        requireUuid(conversationId, "conversation ID");
        requireUuid(recipientUserId, "recipient user ID");
        if (metadata == null || receipt == null || !receipt.matches(metadata))
            throw new IOException("Invalid image upload receipt");
        ConnectionManager active;
        synchronized (lock) {
            active = connection;
            if (state != State.READY || active == null || !active.isConnected())
                throw new IOException("Image session is not connected");
        }
        bridge.sendImage(conversationId, recipientUserId, metadata, receipt, active);
    }

    /** Send private video/file metadata only after the exact ciphertext receipt. */
    public void sendLargeFile(String conversationId, String recipientUserId,
            AndroidLargeFileTransfer.Metadata metadata,
            AndroidLargeFileSession.UploadReceipt receipt) throws Exception {
        requireUuid(conversationId, "conversation ID");
        requireUuid(recipientUserId, "recipient user ID");
        if (metadata == null || receipt == null || !receipt.matches(metadata))
            throw new IOException("Invalid large-file upload receipt");
        ConnectionManager active;
        synchronized (lock) {
            active = connection;
            if (state != State.READY || active == null || !active.isConnected())
                throw new IOException("Large-file session is not connected");
        }
        bridge.sendLargeFile(conversationId, recipientUserId, metadata, receipt, active);
    }

    public void stop() {
        ConnectionManager active;
        synchronized (lock) {
            state = State.STOPPED;
            active = connection;
            connection = null;
        }
        if (active != null) active.shutdown();
        listener.onState(State.STOPPED);
    }

    private void updateState(State next) {
        synchronized (lock) {
            if (state == State.STOPPED && next != State.STOPPED) return;
            state = next;
        }
        listener.onState(next);
    }

    private boolean coreFailed() {
        return state == State.FAILED;
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
}
