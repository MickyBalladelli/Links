package ai.links.app;

import java.io.IOException;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Runs one bounded mailbox recovery through the existing reconnecting WebSocket.
 * The bridge owns protobuf framing and calls links-client-core::receive.
 */
public final class AndroidMissingMessageRecovery implements MissingMessageRecovery {
    public static final long RECOVERY_TIMEOUT_MS = 25_000;

    public interface FrameBridge {
        long durableCursor() throws Exception;
        byte[] createHello(String deviceId, long durableCursor) throws Exception;
        /** Return true only after replay is drained and QueueAck is sent. */
        boolean handleServerFrame(byte[] frame, ConnectionManager connection, boolean fullSync)
                throws Exception;
    }

    private final String endpoint;
    private final ClientSession session;
    private final FrameBridge bridge;

    public AndroidMissingMessageRecovery(
            String endpoint,
            ClientSession session,
            FrameBridge bridge) {
        this.endpoint = endpoint;
        this.session = session;
        this.bridge = bridge;
    }

    @Override
    public Outcome recover(long cursorHint, boolean fullSync) throws Exception {
        if (!session.isAuthenticated() || bridge == null) {
            return Outcome.AUTHENTICATION_REQUIRED;
        }
        long durableCursor = bridge.durableCursor();
        if (durableCursor < 0) return Outcome.RETRY;

        CountDownLatch finished = new CountDownLatch(1);
        AtomicBoolean completed = new AtomicBoolean();
        AtomicReference<Throwable> failure = new AtomicReference<>();
        final ConnectionManager[] holder = new ConnectionManager[1];
        ConnectionManager connection = new ConnectionManager(
                endpoint,
                () -> bridge.createHello(session.deviceId(), durableCursor),
                new ConnectionManager.Listener() {
                    @Override
                    public void onConnected() {}

                    @Override
                    public void onBinaryFrame(byte[] frame) {
                        try {
                            if (bridge.handleServerFrame(frame, holder[0], fullSync)
                                    && completed.compareAndSet(false, true)) {
                                finished.countDown();
                                holder[0].stop();
                            }
                        } catch (Exception error) {
                            failure.compareAndSet(null, error);
                            if (completed.compareAndSet(false, true)) {
                                finished.countDown();
                                holder[0].stop();
                            }
                        }
                    }

                    @Override
                    public void onDisconnected() {}

                    @Override
                    public void onFailure() {}
                });
        holder[0] = connection;
        try {
            connection.start();
            if (!finished.await(RECOVERY_TIMEOUT_MS, TimeUnit.MILLISECONDS))
                return Outcome.RETRY;
            return failure.get() == null && completed.get() ? Outcome.COMPLETE : Outcome.RETRY;
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            return Outcome.RETRY;
        } finally {
            connection.shutdown();
        }
    }
}
