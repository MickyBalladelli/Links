package ai.links.identity;

import android.content.Context;
import java.io.IOException;
import java.nio.ByteBuffer;
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
    public KeyReference createIdentity() throws GeneralSecurityException, IOException {
        synchronized (WORKER_LOCK) {
            byte[] reference = NativeIdentityBridge.create(vault);
            return new KeyReference(new String(reference, 0, 36, StandardCharsets.US_ASCII),
                    Arrays.copyOfRange(reference, 36, 68));
        }
    }
    /** Restore/check only. Missing or invalidated keys never trigger creation. */
    public void validateIdentity(KeyReference identity) throws GeneralSecurityException, IOException {
        synchronized (WORKER_LOCK) {
            if (!Arrays.equals(identity.publicKey,
                    NativeIdentityBridge.publicKey(vault, identity.handleBytes())))
                throw new GeneralSecurityException("Identity authentication failed");
        }
    }
    public byte[] sign(KeyReference identity, byte[] transcript) throws GeneralSecurityException, IOException {
        if (transcript == null || transcript.length > 1024 * 1024)
            throw new GeneralSecurityException("Invalid identity transcript");
        synchronized (WORKER_LOCK) {
            return NativeIdentityBridge.sign(vault, identity.handleBytes(), identity.publicKey, transcript);
        }
    }
    /** Build the exact phone-auth proof signed by the identity key. */
    public byte[] phoneAuthTranscript(KeyReference identity, String phone, String channel,
            UUID deviceId, UUID mlsNodeId) throws GeneralSecurityException {
        if (identity == null || phone == null || channel == null || deviceId == null || mlsNodeId == null)
            throw new GeneralSecurityException("Invalid phone-auth input");
        byte[] phoneBytes = phone.getBytes(StandardCharsets.UTF_8);
        byte[] channelBytes = channel.getBytes(StandardCharsets.UTF_8);
        if (phoneBytes.length > 16 || channelBytes.length > 8)
            throw new GeneralSecurityException("Invalid phone-auth input");
        byte[] transcript = NativeIdentityBridge.phoneAuthTranscript(phoneBytes, channelBytes,
                uuidBytes(deviceId), uuidBytes(mlsNodeId), identity.publicKey());
        if (transcript == null || transcript.length == 0)
            throw new GeneralSecurityException("Invalid phone-auth transcript");
        return transcript;
    }
    /** Build the nonce- and MLS-credential-bound enrollment proof. */
    public byte[] enrollmentTranscript(KeyReference identity, UUID userId, UUID deviceId,
            UUID mlsNodeId, UUID challengeId, byte[] nonce, long expiresAtMs,
            byte[] mlsCredential) throws GeneralSecurityException {
        if (identity == null || userId == null || deviceId == null || mlsNodeId == null
                || challengeId == null || nonce == null || nonce.length != 32
                || mlsCredential == null || mlsCredential.length > 1024 || expiresAtMs <= 0)
            throw new GeneralSecurityException("Invalid enrollment input");
        byte[] transcript = NativeIdentityBridge.enrollmentTranscript(uuidBytes(userId),
                uuidBytes(deviceId), uuidBytes(mlsNodeId), identity.publicKey(),
                uuidBytes(challengeId), nonce.clone(), expiresAtMs, mlsCredential.clone());
        if (transcript == null || transcript.length == 0)
            throw new GeneralSecurityException("Invalid enrollment transcript");
        return transcript;
    }
    /** Explicit device removal only; never delete on a network retry/login. */
    public void deleteIdentity(KeyReference identity) throws GeneralSecurityException, IOException {
        synchronized (WORKER_LOCK) { NativeIdentityBridge.delete(vault, identity.handleBytes()); }
    }

    private static byte[] uuidBytes(UUID value) {
        return ByteBuffer.allocate(16)
                .putLong(value.getMostSignificantBits())
                .putLong(value.getLeastSignificantBits())
                .array();
    }
}
