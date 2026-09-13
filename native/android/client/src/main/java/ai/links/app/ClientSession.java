package ai.links.app;

import ai.links.identity.HardwareIdentityStore;
import android.content.Context;
import android.content.SharedPreferences;
import android.util.Base64;
import java.io.IOException;
import java.security.GeneralSecurityException;
import java.util.Arrays;
import java.util.UUID;

/**
 * Small durable Android client session.
 *
 * Only hardware-vault handle, public identity key, and public device metadata
 * are stored here. Private seed and bearer token remain out of persistent app
 * state.
 */
public final class ClientSession {
    private static final String PREFS = "links_client_session";
    private static final String HANDLE = "identity_handle";
    private static final String PUBLIC_KEY = "identity_public_key";
    private static final String DEVICE_ID = "device_id";
    private static final String MLS_NODE_ID = "mls_node_id";
    private static final String USER_ID = "user_id";

    private final HardwareIdentityStore identityStore;
    private final SharedPreferences preferences;
    private HardwareIdentityStore.KeyReference identity;
    private String deviceId;
    private String mlsNodeId;
    private String userId;
    private String accessToken;
    private long accessTokenExpiresAtMs;

    public ClientSession(Context context) throws GeneralSecurityException, IOException {
        Context appContext = context.getApplicationContext();
        identityStore = new HardwareIdentityStore(appContext);
        preferences = appContext.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
        restoreExistingIdentity();
    }

    public synchronized boolean isEnrolled() {
        return identity != null;
    }

    public synchronized String deviceId() {
        return deviceId;
    }

    public synchronized String mlsNodeId() {
        return mlsNodeId;
    }

    public synchronized String userId() {
        return userId;
    }

    public synchronized boolean isAuthenticated() {
        return accessToken != null && accessTokenExpiresAtMs > System.currentTimeMillis();
    }

    public synchronized HardwareIdentityStore.KeyReference identity() {
        return identity;
    }

    public synchronized HardwareIdentityStore.KeyReference createIdentity()
            throws GeneralSecurityException, IOException {
        if (identity != null) throw new GeneralSecurityException("Identity already enrolled");
        HardwareIdentityStore.KeyReference created = identityStore.createIdentity();
        String createdDeviceId = UUID.randomUUID().toString();
        String createdMlsNodeId = UUID.randomUUID().toString();
        boolean saved = preferences.edit()
                .putString(HANDLE, created.handle())
                .putString(PUBLIC_KEY, Base64.encodeToString(created.publicKey(), Base64.NO_WRAP))
                .putString(DEVICE_ID, createdDeviceId)
                .putString(MLS_NODE_ID, createdMlsNodeId)
                .commit();
        if (!saved) {
            try {
                identityStore.deleteIdentity(created);
            } catch (GeneralSecurityException cleanup) {
                throw new IOException("Identity metadata unavailable", cleanup);
            }
            throw new IOException("Identity metadata unavailable");
        }
        identity = created;
        deviceId = createdDeviceId;
        mlsNodeId = createdMlsNodeId;
        return created;
    }

    public synchronized void validateIdentity() throws GeneralSecurityException, IOException {
        if (identity == null) throw new GeneralSecurityException("Identity is not enrolled");
        identityStore.validateIdentity(identity);
    }

    public synchronized OtpClient.Challenge startOtp(OtpClient api, String phone, String channel)
            throws GeneralSecurityException, IOException {
        if (api == null || identity == null || deviceId == null || mlsNodeId == null)
            throw new GeneralSecurityException("Identity is not ready for OTP");
        byte[] transcript = null;
        byte[] signature = null;
        try {
            transcript = identityStore.phoneAuthTranscript(identity, phone, channel,
                    UUID.fromString(deviceId), UUID.fromString(mlsNodeId));
            signature = identityStore.sign(identity, transcript);
            OtpClient.Challenge challenge = api.start(phone, channel, deviceId, mlsNodeId,
                    OtpClient.encode(identity.publicKey()), OtpClient.encode(signature));
            validateChallenge(challenge);
            return challenge;
        } finally {
            wipe(transcript);
            wipe(signature);
        }
    }

    public synchronized OtpClient.Session finishOtp(OtpClient api, OtpClient.Challenge challenge,
            String code) throws GeneralSecurityException, IOException {
        if (api == null || identity == null) throw new GeneralSecurityException("Identity is not ready for OTP");
        validateChallenge(challenge);
        byte[] transcript = null;
        byte[] signature = null;
        try {
            transcript = identityStore.enrollmentTranscript(identity, UUID.fromString(challenge.userId()),
                    UUID.fromString(deviceId), UUID.fromString(mlsNodeId),
                    UUID.fromString(challenge.challengeId()), challenge.nonceBytes(),
                    challenge.expiresAtMs(), challenge.mlsCredentialBytes());
            signature = identityStore.sign(identity, transcript);
            OtpClient.Session authenticated = api.finish(challenge.challengeId(), code,
                    OtpClient.encode(signature));
            if (!userIdEquals(authenticated.userId(), challenge.userId())
                    || !userIdEquals(authenticated.deviceId(), deviceId))
                throw new GeneralSecurityException("Invalid OTP session binding");
            if (!preferences.edit().putString(USER_ID, authenticated.userId()).commit())
                throw new IOException("Account metadata unavailable");
            userId = authenticated.userId();
            accessToken = authenticated.accessToken();
            accessTokenExpiresAtMs = authenticated.expiresAtMs();
            return authenticated;
        } finally {
            wipe(transcript);
            wipe(signature);
        }
    }

    private void restoreExistingIdentity() throws GeneralSecurityException {
        String handle = preferences.getString(HANDLE, null);
        String encodedPublicKey = preferences.getString(PUBLIC_KEY, null);
        String storedDeviceId = preferences.getString(DEVICE_ID, null);
        String storedMlsNodeId = preferences.getString(MLS_NODE_ID, null);
        String storedUserId = preferences.getString(USER_ID, null);
        if (handle == null && encodedPublicKey == null && storedDeviceId == null
                && storedMlsNodeId == null && storedUserId == null) return;
        if (handle == null || encodedPublicKey == null || storedDeviceId == null)
            throw new GeneralSecurityException("Incomplete client identity metadata");
        if (storedMlsNodeId == null) {
            if (storedUserId != null) throw new GeneralSecurityException("Incomplete account metadata");
            storedMlsNodeId = UUID.randomUUID().toString();
            if (!preferences.edit().putString(MLS_NODE_ID, storedMlsNodeId).commit())
                throw new GeneralSecurityException("Client identity metadata unavailable");
        }
        byte[] publicKey;
        try {
            publicKey = Base64.decode(encodedPublicKey, Base64.DEFAULT);
        } catch (IllegalArgumentException error) {
            throw new GeneralSecurityException("Invalid client identity metadata", error);
        }
        HardwareIdentityStore.KeyReference restored =
                new HardwareIdentityStore.KeyReference(handle, publicKey);
        identity = restored;
        deviceId = parseDeviceId(storedDeviceId);
        mlsNodeId = parseDeviceId(storedMlsNodeId);
        if (storedUserId != null) userId = parseDeviceId(storedUserId);
    }

    private void validateChallenge(OtpClient.Challenge challenge) throws GeneralSecurityException {
        if (challenge == null || !userIdEquals(challenge.deviceId(), deviceId)
                || !userIdEquals(challenge.mlsNodeId(), mlsNodeId)
                || !Arrays.equals(challenge.publicKeyBytes(), identity.publicKey()))
            throw new GeneralSecurityException("OTP challenge identity mismatch");
    }

    private static boolean userIdEquals(String first, String second) {
        return first != null && first.equals(second);
    }

    private static void wipe(byte[] bytes) {
        if (bytes != null) Arrays.fill(bytes, (byte) 0);
    }

    private static String parseDeviceId(String value) throws GeneralSecurityException {
        try {
            if (!UUID.fromString(value).toString().equals(value)) throw new IllegalArgumentException();
            return value;
        } catch (IllegalArgumentException error) {
            throw new GeneralSecurityException("Invalid device identity metadata", error);
        }
    }
}
