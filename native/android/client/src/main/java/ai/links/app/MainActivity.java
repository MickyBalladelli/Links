package ai.links.app;

import android.app.Activity;
import android.text.Editable;
import android.os.Bundle;
import android.text.InputType;
import android.text.TextWatcher;
import android.view.View;
import android.widget.ArrayAdapter;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.ScrollView;
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
    private Button saveDisplayNameButton;
    private Button changeUsernameButton;
    private TextView usernameNote;
    private TextView displayNameNote;
    private EditText phoneInput;
    private EditText codeInput;
    private EditText displayNameInput;
    private EditText usernameInput;
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
        codeInput.setSaveEnabled(false);
        layout.addView(codeInput);

        verifyButton = new Button(this);
        verifyButton.setText("Verify phone");
        verifyButton.setOnClickListener(view -> verifyOtp());
        layout.addView(verifyButton);

        displayNameInput = new EditText(this);
        displayNameInput.setHint("Display name");
        displayNameInput.setInputType(InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_CAP_WORDS
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS);
        displayNameInput.setSingleLine(true);
        displayNameInput.setVisibility(View.GONE);
        layout.addView(displayNameInput);

        displayNameNote = new TextView(this);
        displayNameNote.setText("Shown in this client. Your username and saved profile stay the same.");
        displayNameNote.setTextSize(12);
        displayNameNote.setVisibility(View.GONE);
        layout.addView(displayNameNote);

        saveDisplayNameButton = new Button(this);
        saveDisplayNameButton.setText("Save display name");
        saveDisplayNameButton.setOnClickListener(view -> saveDisplayName());
        saveDisplayNameButton.setVisibility(View.GONE);
        layout.addView(saveDisplayNameButton);

        usernameInput = new EditText(this);
        usernameInput.setHint("New username");
        usernameInput.setInputType(InputType.TYPE_CLASS_TEXT
                | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS
                | InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD);
        usernameInput.setSingleLine(true);
        usernameInput.setVisibility(View.GONE);
        layout.addView(usernameInput);

        usernameNote = new TextView(this);
        usernameNote.setText("Use 3–32 lowercase letters, numbers, or underscores. Your old username becomes available to others.");
        usernameNote.setTextSize(12);
        usernameNote.setVisibility(View.GONE);
        layout.addView(usernameNote);

        changeUsernameButton = new Button(this);
        changeUsernameButton.setText("Change username");
        changeUsernameButton.setOnClickListener(view -> changeUsername());
        changeUsernameButton.setVisibility(View.GONE);
        layout.addView(changeUsernameButton);

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
        ScrollView scrollView = new ScrollView(this);
        scrollView.setFillViewport(true);
        scrollView.addView(layout);
        setContentView(scrollView);

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
        pendingChallenge = null;
        codeInput.setText("");
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
                try {
                    session.saveProfileUsername(otpClient.currentUsername(session.accessToken()));
                } catch (Exception ignored) {
                    // Keep the saved profile name if the optional refresh is unavailable.
                }
                runOnUiThread(() -> {
                    pendingChallenge = null;
                    codeInput.setText("");
                    refreshStatus();
                });
            } catch (Exception error) {
                runOnUiThread(() -> {
                    refreshStatus();
                    showError(error);
                });
            }
        });
    }

    private void changeUsername() {
        if (session == null || otpClient == null || !session.isAuthenticated()) return;
        String requestedHandle = usernameInput.getText().toString();
        changeUsernameButton.setEnabled(false);
        usernameInput.setEnabled(false);
        status.setText("Changing username…");
        identityWorker.execute(() -> {
            try {
                String handle = otpClient.changeUsername(session.accessToken(), requestedHandle);
                session.saveProfileUsername(handle);
                runOnUiThread(() -> {
                    usernameInput.setText(handle);
                    refreshStatus();
                    status.setText("Username changed to @" + handle);
                });
            } catch (Exception error) {
                runOnUiThread(() -> {
                    refreshStatus();
                    status.setText("Username not changed\n" + error.getMessage());
                });
            }
        });
    }

    private void saveDisplayName() {
        if (session == null || !session.isAuthenticated()) return;
        try {
            session.saveProfileDisplayName(displayNameInput.getText().toString());
            refreshStatus();
            status.setText("Display name saved on this device");
        } catch (Exception error) {
            status.setText("Display name not saved\n" + error.getMessage());
        }
    }

    private void refreshStatus() {
        boolean enrolled = session != null && session.isEnrolled();
        boolean authenticated = enrolled && session.isAuthenticated();
        boolean usernameWasVisible = usernameInput.getVisibility() == View.VISIBLE;
        boolean displayNameWasVisible = displayNameInput.getVisibility() == View.VISIBLE;
        createButton.setEnabled(session != null && !enrolled);
        sendCodeButton.setEnabled(enrolled && !authenticated);
        codeInput.setEnabled(enrolled && !authenticated && pendingChallenge != null);
        verifyButton.setEnabled(enrolled && !authenticated && pendingChallenge != null
                && codeInput.getText().length() > 0);
        displayNameInput.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        displayNameNote.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        saveDisplayNameButton.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        usernameInput.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        usernameNote.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        changeUsernameButton.setVisibility(authenticated ? View.VISIBLE : View.GONE);
        usernameInput.setEnabled(authenticated);
        changeUsernameButton.setEnabled(authenticated);
        displayNameInput.setEnabled(authenticated);
        saveDisplayNameButton.setEnabled(authenticated);
        if (authenticated && !displayNameWasVisible)
            displayNameInput.setText(session.profileDisplayName().isEmpty()
                    ? session.profileUsername() : session.profileDisplayName());
        if (authenticated && !usernameWasVisible)
            usernameInput.setText(session.profileUsername());
        if (authenticated) {
            String username = session.profileUsername();
            String usernameLine = username.isEmpty() ? "" : "\nUsername: @" + username;
            String displayName = session.profileDisplayName().isEmpty()
                    ? username : session.profileDisplayName();
            String displayNameLine = displayName.isEmpty() ? "" : "\nDisplay name: " + displayName;
            status.setText("Phone verified\nAccount: " + session.userId()
                    + displayNameLine + usernameLine);
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
