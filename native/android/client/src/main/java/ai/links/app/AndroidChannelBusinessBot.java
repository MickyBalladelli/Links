package ai.links.app;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.UUID;

/**
 * Shared-core host for channel, business, and bot surfaces on Android.
 *
 * The surface only selects policy and routing. MLS, encryption, cursors,
 * replay, and delivery receipts stay in AndroidTextMessaging.CoreBridge.
 */
public final class AndroidChannelBusinessBot {
    public static final int MAX_SURFACE_NAME_BYTES = 80;
    public static final int MAX_TEXT_BYTES = 64 * 1024;

    public enum State { STOPPED, CONNECTING, READY, FAILED }
    public enum Kind { CHANNEL, BUSINESS, BOT }
    public enum Role { OWNER, ADMIN, MEMBER, SUBSCRIBER, BOT }

    public static final class SurfaceProfile {
        private final String surfaceId;
        private final Kind kind;
        private final Role role;
        private final String displayName;
        private final boolean verified;

        public SurfaceProfile(String surfaceId, Kind kind, Role role, String displayName,
                boolean verified) throws IOException {
            requireUuid(surfaceId, "surface ID");
            if (kind == null || role == null || displayName == null
                    || displayName.isEmpty()
                    || displayName.getBytes(StandardCharsets.UTF_8).length > MAX_SURFACE_NAME_BYTES
                    || hasControlCharacter(displayName)
                    || !roleAllowed(kind, role)) {
                throw new IOException("Invalid surface profile");
            }
            this.surfaceId = surfaceId;
            this.kind = kind;
            this.role = role;
            this.displayName = displayName;
            this.verified = verified;
        }

        public String surfaceId() { return surfaceId; }
        public Kind kind() { return kind; }
        public Role role() { return role; }
        public String displayName() { return displayName; }
        public boolean verified() { return verified; }
        public boolean canSend() {
            return kind != Kind.CHANNEL || role != Role.SUBSCRIBER;
        }
        public boolean canPublish() {
            return kind == Kind.CHANNEL && (role == Role.OWNER || role == Role.ADMIN)
                    || kind == Kind.BUSINESS && (role == Role.OWNER || role == Role.ADMIN);
        }
        public boolean canManage() {
            return role == Role.OWNER || role == Role.ADMIN;
        }
    }

    public interface Listener {
        void onState(State state);
        void onSurfaceMessage(SurfaceProfile surface, String conversationId,
                String senderDeviceId, String text, long sequenceId, long sentAtMs);
        void onFailure();
    }

    private final Object lock = new Object();
    private final String endpoint;
    private final ClientSession session;
    private final SurfaceProfile surface;
    private final AndroidTextMessaging.CoreBridge bridge;
    private final Listener listener;
    private ConnectionManager connection;
    private State state = State.STOPPED;

    public AndroidChannelBusinessBot(String endpoint, ClientSession session,
            SurfaceProfile surface, AndroidTextMessaging.CoreBridge bridge, Listener listener)
            throws IOException {
        if (endpoint == null || endpoint.isEmpty() || session == null || surface == null
                || bridge == null || listener == null) {
            throw new IOException("Invalid surface session");
        }
        this.endpoint = endpoint;
        this.session = session;
        this.surface = surface;
        this.bridge = bridge;
        this.listener = listener;
    }

    public SurfaceProfile surface() { return surface; }

    public State state() {
        synchronized (lock) { return state; }
    }

    public boolean isConnected() {
        synchronized (lock) {
            return state == State.READY && connection != null && connection.isConnected();
        }
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
                () -> bridge.createHello(session.deviceId(), session.accessToken(),
                        bridge.durableCursor()),
                new ConnectionManager.Listener() {
                    @Override
                    public void onConnected() { updateState(State.READY); }

                    @Override
                    public void onBinaryFrame(byte[] frame) {
                        try {
                            bridge.handleServerFrame(frame, holder[0], new AndroidTextMessaging.Listener() {
                                @Override
                                public void onState(AndroidTextMessaging.State ignored) { }

                                @Override
                                public void onTextMessage(String conversationId,
                                        String senderDeviceId, String text,
                                        long sequenceId, long sentAtMs) {
                                    listener.onSurfaceMessage(surface, conversationId, senderDeviceId,
                                            text, sequenceId, sentAtMs);
                                }

                                @Override
                                public void onFailure() { fail(); }
                            });
                        } catch (Exception error) {
                            fail();
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
                    public void onFailure() { fail(); }
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

    public void sendText(String conversationId, String text) throws Exception {
        requireUuid(conversationId, "conversation ID");
        if (!surface.canSend() || text == null || text.isEmpty()
                || text.getBytes(StandardCharsets.UTF_8).length > MAX_TEXT_BYTES) {
            throw new IOException("Surface cannot send this text");
        }
        ConnectionManager active;
        synchronized (lock) {
            active = connection;
            if (state != State.READY || active == null || !active.isConnected()) {
                throw new IOException("Surface session is not connected");
            }
        }
        bridge.sendSurfaceText(surface.surfaceId(), conversationId, text, active);
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

    private void fail() {
        synchronized (lock) { state = State.FAILED; }
        listener.onFailure();
    }

    private static boolean roleAllowed(Kind kind, Role role) {
        return switch (kind) {
            case CHANNEL -> role == Role.OWNER || role == Role.ADMIN || role == Role.SUBSCRIBER;
            case BUSINESS -> role == Role.OWNER || role == Role.ADMIN || role == Role.MEMBER;
            case BOT -> role == Role.BOT;
        };
    }

    private static boolean hasControlCharacter(String value) {
        for (int index = 0; index < value.length(); index++) {
            if (Character.isISOControl(value.charAt(index))) return true;
        }
        return false;
    }

    private static void requireUuid(String value, String name) throws IOException {
        try {
            UUID uuid = UUID.fromString(value);
            if (uuid.equals(new UUID(0, 0)) || !uuid.toString().equals(value)) {
                throw new IllegalArgumentException();
            }
        } catch (IllegalArgumentException | NullPointerException error) {
            throw new IOException("Invalid " + name, error);
        }
    }
}
