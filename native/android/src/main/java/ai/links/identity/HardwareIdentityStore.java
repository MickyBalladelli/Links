package ai.links.identity;

import android.content.Context;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.util.Arrays;
import java.util.UUID;

/** Rust Ed25519 signer using hardware-wrapped seeds, not Ed25519 signing in TEE.
 * Invoke off the main thread. No seed is retained between operations. */
public final class HardwareIdentityStore {
    private static final Object WORKER_LOCK = new Object();
    private final HardwareSeedVault vault;

    public HardwareIdentityStore(Context context) throws GeneralSecurityException, IOException {
        vault = new HardwareSeedVault(context);
    }
    /** Public metadata only; persist with the authenticated account/device binding. */
    public static final class KeyReference {
        private final String handle;
        private final byte[] publicKey;
        public KeyReference(String handle, byte[] publicKey) throws GeneralSecurityException {
            try {
                if (handle == null || handle.length() != 36 || !UUID.fromString(handle).toString().equals(handle)
                        || publicKey == null || publicKey.length != 32) throw new IllegalArgumentException();
            } catch (IllegalArgumentException error) {
                throw new GeneralSecurityException("Invalid identity reference");
            }
            this.handle = handle;
            this.publicKey = publicKey.clone();
        }
        public String handle() { return handle; }
        public byte[] publicKey() { return publicKey.clone(); }
        private byte[] handleBytes() { return handle.getBytes(StandardCharsets.US_ASCII); }
    }
    public KeyReference createIdentity() throws GeneralSecurityException {
        synchronized (WORKER_LOCK) {
            byte[] reference = NativeIdentityBridge.create(vault);
            return new KeyReference(new String(reference, 0, 36, StandardCharsets.US_ASCII),
                    Arrays.copyOfRange(reference, 36, 68));
        }
    }
    /** Restore/check only. Missing or invalidated keys never trigger creation. */
    public void validateIdentity(KeyReference identity) throws GeneralSecurityException {
        synchronized (WORKER_LOCK) {
            if (!Arrays.equals(identity.publicKey,
                    NativeIdentityBridge.publicKey(vault, identity.handleBytes())))
                throw new GeneralSecurityException("Identity authentication failed");
        }
    }
    public byte[] sign(KeyReference identity, byte[] transcript) throws GeneralSecurityException {
        if (transcript == null || transcript.length > 1024 * 1024)
            throw new GeneralSecurityException("Invalid identity transcript");
        synchronized (WORKER_LOCK) {
            return NativeIdentityBridge.sign(vault, identity.handleBytes(), identity.publicKey, transcript);
        }
    }
    /** Explicit device removal only; never delete on a network retry/login. */
    public void deleteIdentity(KeyReference identity) throws GeneralSecurityException {
        synchronized (WORKER_LOCK) { NativeIdentityBridge.delete(vault, identity.handleBytes()); }
    }
}
