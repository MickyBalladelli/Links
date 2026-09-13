package ai.links.app;

import android.app.Activity;
import android.text.Editable;
import android.os.Bundle;
import android.text.InputType;
import android.text.TextWatcher;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.Spinner;
import android.widget.TextView;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Minimal onboarding shell. Hardware and network operations run off the UI thread. */
public final class MainActivity extends Activity {
    private final ExecutorService identityWorker = Executors.newSingleThreadExecutor();
    private TextView status;
    private Button createButton;
    private Button sendCodeButton;
    private Button verifyButton;
    private EditText phoneInput;
    private EditText codeInput;
    private Spinner channelInput;
    private ClientSession session;
    private OtpClient otpClient;
    private OtpClient.Challenge pendingChallenge;

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

        phoneInput = new EditText(this);
        phoneInput.setHint("Phone, e.g. +12025550123");
        phoneInput.setInputType(InputType.TYPE_CLASS_PHONE);
        phoneInput.setSingleLine(true);
        layout.addView(phoneInput);

        channelInput = new Spinner(this);
        ArrayAdapter<String> channels = new ArrayAdapter<>(this,
                android.R.layout.simple_spinner_item, new String[] {"sms", "whatsapp"});
        channels.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
        channelInput.setAdapter(channels);
        layout.addView(channelInput);

        createButton = new Button(this);
        createButton.setText("Create hardware identity");
        createButton.setOnClickListener(view -> createIdentity());
        layout.addView(createButton);

        sendCodeButton = new Button(this);
        sendCodeButton.setText("Send OTP");
        sendCodeButton.setOnClickListener(view -> sendOtp());
        layout.addView(sendCodeButton);

        codeInput = new EditText(this);
        codeInput.setHint("Verification code");
        codeInput.setInputType(InputType.TYPE_CLASS_NUMBER);
        codeInput.setSingleLine(true);
        layout.addView(codeInput);

        verifyButton = new Button(this);
        verifyButton.setText("Verify phone");
        verifyButton.setOnClickListener(view -> verifyOtp());
        layout.addView(verifyButton);
        codeInput.addTextChangedListener(new TextWatcher() {
            @Override
            public void beforeTextChanged(CharSequence text, int start, int count, int after) {}

            @Override
            public void onTextChanged(CharSequence text, int start, int before, int count) {
                verifyButton.setEnabled(session != null && session.isEnrolled()
                        && !session.isAuthenticated() && pendingChallenge != null
                        && text.length() > 0);
            }

            @Override
            public void afterTextChanged(Editable text) {}
        });
        setContentView(layout);

        createButton.setEnabled(false);
        sendCodeButton.setEnabled(false);
        codeInput.setEnabled(false);
        verifyButton.setEnabled(false);
        status.setText("Opening secure identity store…");
        identityWorker.execute(this::loadSession);
    }

    private void loadSession() {
        try {
            ClientSession loaded = new ClientSession(getApplicationContext());
            session = loaded;
            if (loaded.isEnrolled()) loaded.validateIdentity();
            otpClient = new OtpClient(BuildConfig.AUTH_BASE_URL);
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

    private void sendOtp() {
        if (session == null || !session.isEnrolled() || otpClient == null) return;
        String phone = phoneInput.getText().toString();
        String channel = channelInput.getSelectedItem().toString();
        sendCodeButton.setEnabled(false);
        verifyButton.setEnabled(false);
        status.setText("Sending verification code…");
        identityWorker.execute(() -> {
            try {
                OtpClient.Challenge challenge = session.startOtp(otpClient, phone, channel);
                runOnUiThread(() -> {
                    pendingChallenge = challenge;
                    codeInput.setText("");
                    refreshStatus();
                    status.setText("Code sent. Enter it to finish onboarding.");
                });
            } catch (Exception error) {
                runOnUiThread(() -> {
                    refreshStatus();
                    showError(error);
                });
            }
        });
    }

    private void verifyOtp() {
        if (session == null || otpClient == null || pendingChallenge == null) return;
        String code = codeInput.getText().toString();
        OtpClient.Challenge challenge = pendingChallenge;
        sendCodeButton.setEnabled(false);
        verifyButton.setEnabled(false);
        status.setText("Verifying phone…");
        identityWorker.execute(() -> {
            try {
                session.finishOtp(otpClient, challenge, code);
                runOnUiThread(() -> {
                    pendingChallenge = null;
                    codeInput.setText("");
                    refreshStatus();
                    status.setText("Phone verified\nAccount: " + session.userId());
                });
            } catch (Exception error) {
                runOnUiThread(() -> {
                    refreshStatus();
                    showError(error);
                });
            }
        });
    }

    private void refreshStatus() {
        boolean enrolled = session != null && session.isEnrolled();
        boolean authenticated = enrolled && session.isAuthenticated();
        createButton.setEnabled(session != null && !enrolled);
        sendCodeButton.setEnabled(enrolled && !authenticated);
        codeInput.setEnabled(enrolled && !authenticated && pendingChallenge != null);
        verifyButton.setEnabled(enrolled && !authenticated && pendingChallenge != null
                && codeInput.getText().length() > 0);
        if (authenticated) {
            status.setText("Phone verified\nAccount: " + session.userId());
        } else if (enrolled) {
            status.setText("Identity ready\nDevice: " + session.deviceId()
                    + "\nVerify phone to continue");
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
