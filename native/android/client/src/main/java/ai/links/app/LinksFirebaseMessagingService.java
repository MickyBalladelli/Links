package ai.links.app;

import com.google.firebase.messaging.FirebaseMessagingService;
import com.google.firebase.messaging.RemoteMessage;

/** Receives only data-only mailbox wakeups; all message content stays on TLS. */
public final class LinksFirebaseMessagingService extends FirebaseMessagingService {
    @Override
    public void onMessageReceived(RemoteMessage message) {
        if (message == null) return;
        try {
            FcmWakeup wakeup = FcmWakeup.parse(message.getData());
            ClientSession session = new ClientSession(getApplicationContext());
            if (!wakeup.recipientDeviceId().equals(session.deviceId())) return;
            FcmRecoveryScheduler.enqueue(this, wakeup);
        } catch (Exception error) {
            // Push data is untrusted. Ignore malformed or stale wakeups.
        }
    }

    @Override
    public void onDeletedMessages() {
        try {
            ClientSession session = new ClientSession(getApplicationContext());
            if (session.deviceId() != null)
                FcmRecoveryScheduler.enqueue(this, FcmWakeup.fullSync(session.deviceId()));
        } catch (Exception error) {
            // The next authenticated app start must perform recovery.
        }
    }

    @Override
    public void onNewToken(String token) {
        if (token == null || token.isEmpty()) return;
        ((LinksApplication) getApplicationContext()).handleFcmToken(token);
    }
}
