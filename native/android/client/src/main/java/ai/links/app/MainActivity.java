package ai.links.app;

import android.app.Activity;
import android.os.Bundle;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Minimal first-run shell. All hardware operations run off the UI thread. */
public final class MainActivity extends Activity {
    private final ExecutorService identityWorker = Executors.newSingleThreadExecutor();
    private TextView status;
    private Button createButton;
    private ClientSession session;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);

        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setPadding(48, 64, 48, 48);

        TextView title = new TextView(this);
        title.setText("Links");
        title.setTextSize(28);
        layout.addView(title);

        status = new TextView(this);
        status.setPadding(0, 32, 0, 32);
        layout.addView(status);

        createButton = new Button(this);
        createButton.setText("Create hardware identity");
        createButton.setOnClickListener(view -> createIdentity());
        layout.addView(createButton);
        setContentView(layout);

        createButton.setEnabled(false);
        status.setText("Opening secure identity store…");
        identityWorker.execute(this::loadSession);
    }

    private void loadSession() {
        try {
            ClientSession loaded = new ClientSession(getApplicationContext());
            session = loaded;
            if (loaded.isEnrolled()) loaded.validateIdentity();
            runOnUiThread(this::refreshStatus);
        } catch (Exception error) {
            runOnUiThread(() -> showError(error));
        }
    }

    private void createIdentity() {
        if (session == null || session.isEnrolled()) return;
        createButton.setEnabled(false);
        status.setText("Creating hardware-backed identity…");
        identityWorker.execute(() -> {
            try {
                session.createIdentity();
                runOnUiThread(this::refreshStatus);
            } catch (Exception error) {
                runOnUiThread(() -> {
                    createButton.setEnabled(true);
                    showError(error);
                });
            }
        });
    }

    private void refreshStatus() {
        createButton.setEnabled(session != null && !session.isEnrolled());
        if (session != null && session.isEnrolled()) {
            status.setText("Identity ready\nDevice: " + session.deviceId());
        } else {
            status.setText("No identity enrolled");
        }
    }

    private void showError(Exception error) {
        status.setText("Identity unavailable\n" + error.getClass().getSimpleName());
    }

    @Override
    protected void onDestroy() {
        identityWorker.shutdownNow();
        super.onDestroy();
    }
}
