package ai.links.app;

import android.app.Application;
import android.content.Context;

/** Application shell over the Rust shared client core and Android hardware vault. */
public final class LinksApplication extends Application {
    public interface MissingMessageRecoveryFactory {
        MissingMessageRecovery create(Context context) throws Exception;
    }

    public interface FcmTokenHandler {
        void onFcmToken(String token) throws Exception;
    }

    private MissingMessageRecoveryFactory recoveryFactory;
    private FcmTokenHandler tokenHandler;

    /** Configure these from the app's core/bootstrap composition root. */
    public synchronized void configurePushAdapters(
            MissingMessageRecoveryFactory recoveryFactory,
            FcmTokenHandler tokenHandler) {
        this.recoveryFactory = recoveryFactory;
        this.tokenHandler = tokenHandler;
    }

    synchronized MissingMessageRecovery createMissingMessageRecovery() throws Exception {
        return recoveryFactory == null ? null : recoveryFactory.create(getApplicationContext());
    }

    synchronized void handleFcmToken(String token) {
        if (tokenHandler == null) return;
        try {
            tokenHandler.onFcmToken(token);
        } catch (Exception error) {
            // Token registration retries on the next token callback or login.
        }
    }
}
