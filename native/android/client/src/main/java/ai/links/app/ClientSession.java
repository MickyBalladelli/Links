package ai.links.app;

import ai.links.identity.HardwareIdentityStore;
import android.content.Context;
import android.content.SharedPreferences;
import android.util.Base64;
import java.io.IOException;
import java.security.GeneralSecurityException;
import java.util.UUID;

/**
 * Small durable Android client session.
 *
 * Only a hardware-vault handle, public identity key, and device ID are stored
 * here. Private seed material remains behind HardwareIdentityStore.
 */
public final class ClientSession {
    private static final String PREFS = "links_client_session";
    private static final String HANDLE = "identity_handle";
    private static final String PUBLIC_KEY = "identity_public_key";
    private static final String DEVICE_ID = "device_id";

    private final HardwareIdentityStore identityStore;
    private final SharedPreferences preferences;
    private HardwareIdentityStore.KeyReference identity;
    private String deviceId;

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

    public synchronized HardwareIdentityStore.KeyReference identity() {
        return identity;
    }

    public synchronized HardwareIdentityStore.KeyReference createIdentity()
            throws GeneralSecurityException, IOException {
        if (identity != null) throw new GeneralSecurityException("Identity already enrolled");
        HardwareIdentityStore.KeyReference created = identityStore.createIdentity();
        String createdDeviceId = UUID.randomUUID().toString();
        boolean saved = preferences.edit()
                .putString(HANDLE, created.handle())
                .putString(PUBLIC_KEY, Base64.encodeToString(created.publicKey(), Base64.NO_WRAP))
                .putString(DEVICE_ID, createdDeviceId)
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
        return created;
    }

    public synchronized void validateIdentity() throws GeneralSecurityException, IOException {
        if (identity == null) throw new GeneralSecurityException("Identity is not enrolled");
        identityStore.validateIdentity(identity);
    }

    private void restoreExistingIdentity() throws GeneralSecurityException {
        String handle = preferences.getString(HANDLE, null);
        String encodedPublicKey = preferences.getString(PUBLIC_KEY, null);
        String storedDeviceId = preferences.getString(DEVICE_ID, null);
        if (handle == null && encodedPublicKey == null && storedDeviceId == null) return;
        if (handle == null || encodedPublicKey == null || storedDeviceId == null)
            throw new GeneralSecurityException("Incomplete client identity metadata");
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
