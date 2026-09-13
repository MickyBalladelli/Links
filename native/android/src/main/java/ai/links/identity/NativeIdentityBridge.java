package ai.links.identity;

import java.security.GeneralSecurityException;
import java.io.IOException;

/** Internal synchronous JNI boundary. No seed-export API. */
final class NativeIdentityBridge {
    static { System.loadLibrary("links_identity_jni"); }
    private NativeIdentityBridge() {}
    static native byte[] create(Object vault) throws GeneralSecurityException, IOException;
    static native byte[] restoreFromRecovery(Object vault, byte[] phrase, byte[] passphrase)
            throws GeneralSecurityException, IOException;
    static native byte[] generateRecoveryMnemonic(int wordCount) throws GeneralSecurityException;
    static native byte[] createFromPasskeyPrf(Object vault, byte[] prfOutput)
            throws GeneralSecurityException, IOException;
    static native byte[] backupWithPasskey(Object vault, byte[] handle, byte[] backupId,
            byte[] deviceId, byte[] credentialId, byte[] salt, byte[] prfOutput)
            throws GeneralSecurityException, IOException;
    static native byte[] restoreFromPasskey(Object vault, byte[] backupId, byte[] deviceId,
            byte[] credentialId, byte[] envelope, byte[] prfOutput)
            throws GeneralSecurityException, IOException;
    static native byte[] publicKey(Object vault, byte[] handle) throws GeneralSecurityException, IOException;
    static native byte[] sign(Object vault, byte[] handle, byte[] publicKey, byte[] transcript) throws GeneralSecurityException, IOException;
    static native void delete(Object vault, byte[] handle) throws GeneralSecurityException, IOException;
    static native byte[] phoneAuthTranscript(byte[] phone, byte[] channel,
            byte[] deviceId, byte[] mlsNodeId, byte[] publicKey) throws GeneralSecurityException;
    static native byte[] enrollmentTranscript(byte[] userId, byte[] deviceId,
            byte[] mlsNodeId, byte[] publicKey, byte[] challengeId, byte[] nonce,
            long expiresAtMs, byte[] mlsCredential) throws GeneralSecurityException;
}
