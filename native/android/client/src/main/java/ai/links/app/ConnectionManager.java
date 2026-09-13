package ai.links.app;

import java.io.IOException;
import java.net.URL;
import java.util.Collections;
import java.util.concurrent.RejectedExecutionException;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.ScheduledFuture;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import okhttp3.ConnectionSpec;
import okhttp3.OkHttpClient;
import okhttp3.Request;
import okhttp3.Response;
import okhttp3.WebSocket;
import okhttp3.WebSocketListener;
import okio.ByteString;

/** TLS WebSocket lifecycle for the binary links.v1 transport. */
public final class ConnectionManager {
    public static final long HEARTBEAT_INTERVAL_MS = 30_000;
    public static final long HELLO_DEADLINE_MS = 5_000;
    private static final long INITIAL_BACKOFF_MS = 1_000;
    private static final long MAX_BACKOFF_MS = 30_000;
    private static final long STABLE_CONNECTION_MS = 30_000;
    private static final int MAX_FRAME_BYTES = 1024 * 1024;
    private static final int CLOSE_PROTOCOL_ERROR = 1002;
    private static final int CLOSE_UNSUPPORTED_DATA = 1003;

    public interface HelloProvider {
        /** Return one complete protobuf ClientFrame containing Hello. */
        byte[] createHello() throws Exception;
    }

    public interface Listener {
        /** Called after the binary Hello frame was queued on the socket. */
        void onConnected();
        /** Called for each complete binary WebSocket message. */
        void onBinaryFrame(byte[] frame);
        /** Called when the current socket ends and reconnect is scheduled. */
        void onDisconnected();
        /** Called for local protocol or socket failures. No error text is logged. */
        void onFailure();
    }

    private final Object lock = new Object();
    private final String endpoint;
    private final HelloProvider helloProvider;
    private final Listener listener;
    private final OkHttpClient client;
    private final ScheduledExecutorService scheduler = Executors.newSingleThreadScheduledExecutor();
    private long backoffMs = INITIAL_BACKOFF_MS;
    private long generation;
    private boolean started;
    private boolean connecting;
    private WebSocket activeSocket;
    private boolean helloQueued;
    private ScheduledFuture<?> reconnectTask;
    private ScheduledFuture<?> stableResetTask;
    private ScheduledFuture<?> helloDeadlineTask;
    private final AtomicBoolean shutdown = new AtomicBoolean();

    public ConnectionManager(String endpoint, HelloProvider helloProvider, Listener listener)
            throws IOException {
        this.endpoint = validateEndpoint(endpoint);
        if (helloProvider == null || listener == null) throw new IOException("Invalid connection callbacks");
        this.helloProvider = helloProvider;
        this.listener = listener;
        this.client = new OkHttpClient.Builder()
                .connectionSpecs(Collections.singletonList(ConnectionSpec.RESTRICTED_TLS))
                .followRedirects(false)
                .followSslRedirects(false)
                .retryOnConnectionFailure(false)
                .readTimeout(0, TimeUnit.MILLISECONDS)
                .pingInterval(HEARTBEAT_INTERVAL_MS, TimeUnit.MILLISECONDS)
                .build();
    }

    public void start() {
        synchronized (lock) {
            ensureLive();
            if (started) return;
            started = true;
            backoffMs = INITIAL_BACKOFF_MS;
            scheduleConnectLocked(0);
        }
    }

    /** Stop reconnecting but keep the manager reusable. */
    public void stop() {
        WebSocket socket;
        synchronized (lock) {
            if (!started && activeSocket == null && !connecting) return;
            started = false;
            generation++;
            connecting = false;
            cancelReconnectLocked();
            cancelStableResetLocked();
            socket = activeSocket;
            activeSocket = null;
            helloQueued = false;
        }
        if (socket != null) socket.close(1000, "client shutdown");
    }

    /** Permanently release timers and the OkHttp dispatcher. */
    public void shutdown() {
        if (!shutdown.compareAndSet(false, true)) return;
        stop();
        scheduler.shutdownNow();
        client.dispatcher().executorService().shutdown();
        client.connectionPool().evictAll();
    }

    public boolean isConnected() {
        synchronized (lock) { return activeSocket != null && helloQueued; }
    }

    /** Send one complete binary protobuf frame. Returns false if no socket exists. */
    public boolean send(byte[] frame) {
        if (frame == null || frame.length == 0 || frame.length > MAX_FRAME_BYTES) return false;
        WebSocket socket;
        synchronized (lock) {
            socket = helloQueued ? activeSocket : null;
        }
        if (socket == null) return false;
        boolean sent = socket.send(ByteString.of(frame));
        if (!sent) socket.close(1013, "send unavailable");
        return sent;
    }

    private void connectNow() {
        final long attempt;
        synchronized (lock) {
            if (!started || activeSocket != null || connecting) return;
            connecting = true;
            attempt = ++generation;
        }
        Request request = new Request.Builder()
                .url(endpoint)
                .header("Sec-WebSocket-Protocol", "links.v1")
                .build();
        try {
            client.newWebSocket(request, new SocketListener(attempt));
        } catch (RuntimeException error) {
            terminate(attempt, null);
            notifyFailure();
        }
    }

    private final class SocketListener extends WebSocketListener {
        private final long attempt;

        SocketListener(long attempt) { this.attempt = attempt; }

        @Override
        public void onOpen(WebSocket socket, Response response) {
            if (response == null || !"links.v1".equals(response.header("Sec-WebSocket-Protocol"))) {
                socket.close(CLOSE_PROTOCOL_ERROR, "unsupported subprotocol");
                if (terminate(attempt, socket)) {
                    notifyFailure();
                    notifyDisconnected();
                }
                return;
            }
            synchronized (lock) {
                if (!started || attempt != generation || !connecting) {
                    socket.close(1000, "stale connection");
                    return;
                }
                connecting = false;
                activeSocket = socket;
                helloQueued = false;
                scheduleHelloDeadlineLocked(attempt, socket);
                scheduleStableResetLocked(attempt, socket);
            }
            byte[] hello;
            try {
                hello = helloProvider.createHello();
            } catch (Exception error) {
                reject(socket, CLOSE_PROTOCOL_ERROR);
                return;
            }
            boolean sent;
            synchronized (lock) {
                if (!started || attempt != generation || activeSocket != socket) return;
                if (hello == null || hello.length == 0 || hello.length > MAX_FRAME_BYTES) {
                    sent = false;
                } else {
                    helloQueued = true;
                    cancelHelloDeadlineLocked();
                    sent = socket.send(ByteString.of(hello));
                }
            }
            if (!sent) {
                reject(socket, CLOSE_PROTOCOL_ERROR);
                return;
            }
            if (!isCurrent(attempt, socket)) return;
            notifyConnected();
        }

        @Override
        public void onMessage(WebSocket socket, ByteString bytes) {
            if (!isCurrent(attempt, socket)) return;
            if (bytes.size() == 0 || bytes.size() > MAX_FRAME_BYTES) {
                reject(socket, CLOSE_PROTOCOL_ERROR);
                return;
            }
            byte[] frame = bytes.toByteArray();
            try {
                listener.onBinaryFrame(frame);
            } catch (RuntimeException error) {
                reject(socket, CLOSE_PROTOCOL_ERROR);
            }
        }

        @Override
        public void onMessage(WebSocket socket, String text) {
            if (!isCurrent(attempt, socket)) return;
            reject(socket, CLOSE_UNSUPPORTED_DATA);
        }

        @Override
        public void onClosing(WebSocket socket, int code, String reason) {
            socket.close(code, reason);
        }

        @Override
        public void onClosed(WebSocket socket, int code, String reason) {
            if (terminate(attempt, socket)) notifyDisconnected();
        }

        @Override
        public void onFailure(WebSocket socket, Throwable error, Response response) {
            if (terminate(attempt, socket)) {
                notifyFailure();
                notifyDisconnected();
            }
        }
    }

    private boolean isCurrent(long attempt, WebSocket socket) {
        synchronized (lock) {
            return started && attempt == generation && activeSocket == socket && helloQueued;
        }
    }

    private void reject(WebSocket socket, int code) {
        socket.close(code, "protocol error");
        if (terminateForSocket(socket)) {
            notifyFailure();
            notifyDisconnected();
        }
    }

    private boolean terminateForSocket(WebSocket socket) {
        synchronized (lock) {
            if (activeSocket != socket) return false;
            generation++;
            activeSocket = null;
            helloQueued = false;
            cancelStableResetLocked();
            cancelHelloDeadlineLocked();
            scheduleReconnectLocked();
            return true;
        }
    }

    private boolean terminate(long attempt, WebSocket socket) {
        synchronized (lock) {
            boolean current = attempt == generation
                    && (connecting || (socket != null && activeSocket == socket));
            if (!current) return false;
            connecting = false;
            activeSocket = null;
            helloQueued = false;
            cancelStableResetLocked();
            cancelHelloDeadlineLocked();
            scheduleReconnectLocked();
            return true;
        }
    }

    private void scheduleConnectLocked(long delayMs) {
        cancelReconnectLocked();
        try {
            reconnectTask = scheduler.schedule(() -> {
                synchronized (lock) { reconnectTask = null; }
                connectNow();
            }, delayMs, TimeUnit.MILLISECONDS);
        } catch (RejectedExecutionException ignored) {
            // shutdown() owns the terminal lifecycle.
        }
    }

    private void scheduleReconnectLocked() {
        if (!started || reconnectTask != null) return;
        long ceiling = Math.min(MAX_BACKOFF_MS, backoffMs);
        long delay = ThreadLocalRandom.current().nextLong(ceiling + 1);
        backoffMs = Math.min(MAX_BACKOFF_MS, Math.max(INITIAL_BACKOFF_MS, backoffMs * 2));
        scheduleConnectLocked(delay);
    }

    private void scheduleStableResetLocked(long attempt, WebSocket socket) {
        cancelStableResetLocked();
        try {
            stableResetTask = scheduler.schedule(() -> {
                synchronized (lock) {
                    if (started && attempt == generation && activeSocket == socket)
                        backoffMs = INITIAL_BACKOFF_MS;
                }
            }, STABLE_CONNECTION_MS, TimeUnit.MILLISECONDS);
        } catch (RejectedExecutionException ignored) {
            // shutdown() owns the terminal lifecycle.
        }
    }

    private void scheduleHelloDeadlineLocked(long attempt, WebSocket socket) {
        cancelHelloDeadlineLocked();
        try {
            helloDeadlineTask = scheduler.schedule(() -> {
                boolean expired;
                synchronized (lock) {
                    expired = started && attempt == generation && activeSocket == socket && !helloQueued;
                }
                if (expired) reject(socket, CLOSE_PROTOCOL_ERROR);
            }, HELLO_DEADLINE_MS, TimeUnit.MILLISECONDS);
        } catch (RejectedExecutionException ignored) {
            // shutdown() owns the terminal lifecycle.
        }
    }

    private void cancelReconnectLocked() {
        if (reconnectTask != null) {
            reconnectTask.cancel(false);
            reconnectTask = null;
        }
    }

    private void cancelStableResetLocked() {
        if (stableResetTask != null) {
            stableResetTask.cancel(false);
            stableResetTask = null;
        }
    }

    private void cancelHelloDeadlineLocked() {
        if (helloDeadlineTask != null) {
            helloDeadlineTask.cancel(false);
            helloDeadlineTask = null;
        }
    }

    private void notifyConnected() {
        try { listener.onConnected(); } catch (RuntimeException ignored) {}
    }

    private void notifyDisconnected() {
        try { listener.onDisconnected(); } catch (RuntimeException ignored) {}
    }

    private void notifyFailure() {
        try { listener.onFailure(); } catch (RuntimeException ignored) {}
    }

    private void ensureLive() {
        if (shutdown.get()) throw new IllegalStateException("Connection manager is shut down");
    }

    private static String validateEndpoint(String value) throws IOException {
        if (value == null || value.isEmpty()) throw new IOException("Invalid gateway endpoint");
        try {
            URL parsed = new URL(value);
            if (!"wss".equalsIgnoreCase(parsed.getProtocol()) || parsed.getHost().isEmpty()
                    || parsed.getUserInfo() != null || parsed.getQuery() != null
                    || parsed.getRef() != null || !"/v1/connect".equals(parsed.getPath()))
                throw new IOException("Invalid gateway endpoint");
            return value;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid gateway endpoint", error);
        }
    }
}
