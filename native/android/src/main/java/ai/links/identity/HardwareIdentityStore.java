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
    public static final int MAX_CREDENTIAL_ID_BYTES = 1024;
    public static final int MAX_BACKUP_ENVELOPE_BYTES = 1152;
    private static final int MAX_RECOVERY_PHRASE_BYTES = 512;
    private static final int MAX_RECOVERY_PASSPHRASE_BYTES = 256;
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
    /** Explicit local BIP-39 restore. The phrase never enters a network API. */
    public KeyReference restoreFromRecovery(String phrase, String passphrase)
            throws GeneralSecurityException, IOException {
        if (phrase == null || passphrase == null)
            throw new GeneralSecurityException("Invalid recovery input");
        byte[] phraseBytes = phrase.getBytes(StandardCharsets.UTF_8);
        byte[] passphraseBytes = passphrase.getBytes(StandardCharsets.UTF_8);
        if (phraseBytes.length == 0 || phraseBytes.length > MAX_RECOVERY_PHRASE_BYTES
                || passphraseBytes.length > MAX_RECOVERY_PASSPHRASE_BYTES)
            throw new GeneralSecurityException("Invalid recovery input");
        try {
            synchronized (WORKER_LOCK) {
                return reference(NativeIdentityBridge.restoreFromRecovery(vault, phraseBytes, passphraseBytes));
            }
        } finally {
            wipe(phraseBytes);
            wipe(passphraseBytes);
        }
    }

    /** Stable WebAuthn PRF salt for passkey-derived identities. */
    public static byte[] passkeyIdentityPrfSalt() throws GeneralSecurityException {
        byte[] salt = NativeIdentityBridge.passkeyIdentityPrfSalt();
        if (salt == null || salt.length != 32)
            throw new GeneralSecurityException("Invalid passkey identity salt");
        return salt;
    }

    /** Generate a local English 12- or 24-word recovery phrase. */
    public String generateRecoveryMnemonic(int wordCount) throws GeneralSecurityException {
        if (wordCount != 12 && wordCount != 24)
            throw new GeneralSecurityException("Recovery phrase must contain 12 or 24 words");
        byte[] phrase = NativeIdentityBridge.generateRecoveryMnemonic(wordCount);
        if (phrase == null || phrase.length == 0)
            throw new GeneralSecurityException("Invalid recovery phrase");
        try {
            return new String(phrase, StandardCharsets.UTF_8);
        } finally {
            wipe(phrase);
        }
    }

    /** Derive and hardware-seal a new identity from a local passkey PRF result. */
    public KeyReference createFromPasskeyPrf(byte[] prfOutput)
            throws GeneralSecurityException, IOException {
        if (prfOutput == null || prfOutput.length != 32)
            throw new GeneralSecurityException("Invalid passkey identity input");
        byte[] prf = prfOutput.clone();
        try {
            synchronized (WORKER_LOCK) {
                return reference(NativeIdentityBridge.createFromPasskeyPrf(vault, prf));
            }
        } finally {
            wipe(prf);
        }
    }
    /** Seal the identity with a WebAuthn PRF result. Only the opaque envelope returns. */
    public byte[] backupWithPasskey(KeyReference identity, UUID backupId, UUID deviceId,
            byte[] credentialId, byte[] salt, byte[] prfOutput)
            throws GeneralSecurityException, IOException {
        validatePasskeyInput(identity, backupId, deviceId, credentialId, salt, prfOutput);
        byte[] prf = prfOutput.clone();
        try {
            synchronized (WORKER_LOCK) {
                return NativeIdentityBridge.backupWithPasskey(vault, identity.handleBytes(),
                        uuidBytes(backupId), uuidBytes(deviceId), credentialId, salt, prf);
            }
        } finally {
            wipe(prf);
        }
    }
    /** Restore an opaque passkey envelope into the native hardware vault. */
    public KeyReference restoreFromPasskey(UUID backupId, UUID deviceId, byte[] credentialId,
            byte[] envelope, byte[] prfOutput)
            throws GeneralSecurityException, IOException {
        if (envelope == null || envelope.length == 0 || envelope.length > MAX_BACKUP_ENVELOPE_BYTES)
            throw new GeneralSecurityException("Invalid passkey backup envelope");
        validatePasskeyInput(null, backupId, deviceId, credentialId, null, prfOutput);
        byte[] prf = prfOutput.clone();
        try {
            synchronized (WORKER_LOCK) {
                return reference(NativeIdentityBridge.restoreFromPasskey(vault,
                        uuidBytes(backupId), uuidBytes(deviceId), credentialId, envelope, prf));
            }
        } finally {
            wipe(prf);
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

    private static KeyReference reference(byte[] encoded) throws GeneralSecurityException {
        if (encoded == null || encoded.length != 68)
            throw new GeneralSecurityException("Invalid identity reference");
        return new KeyReference(new String(encoded, 0, 36, StandardCharsets.US_ASCII),
                Arrays.copyOfRange(encoded, 36, 68));
    }

    private static void validatePasskeyInput(KeyReference identity, UUID backupId, UUID deviceId,
            byte[] credentialId, byte[] salt, byte[] prfOutput) throws GeneralSecurityException {
        if (identity == null && salt != null)
            throw new GeneralSecurityException("Invalid passkey identity");
        if (backupId == null || deviceId == null || backupId.equals(new UUID(0, 0))
                || deviceId.equals(new UUID(0, 0)) || credentialId == null
                || credentialId.length == 0 || credentialId.length > MAX_CREDENTIAL_ID_BYTES
                || prfOutput == null || prfOutput.length != 32)
            throw new GeneralSecurityException("Invalid passkey backup input");
        if (salt != null && salt.length != 32)
            throw new GeneralSecurityException("Invalid passkey backup salt");
    }

    private static void wipe(byte[] bytes) {
        if (bytes != null) Arrays.fill(bytes, (byte) 0);
    }

    private static byte[] uuidBytes(UUID value) {
        return ByteBuffer.allocate(16)
                .putLong(value.getMostSignificantBits())
                .putLong(value.getLeastSignificantBits())
                .array();
    }
}
