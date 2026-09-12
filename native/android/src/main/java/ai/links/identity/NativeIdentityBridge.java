package ai.links.identity;

import java.security.GeneralSecurityException;
import java.io.IOException;

/** Internal synchronous JNI boundary. No seed-export API. */
final class NativeIdentityBridge {
    static { System.loadLibrary("links_identity_jni"); }
    private NativeIdentityBridge() {}
    static native byte[] create(Object vault) throws GeneralSecurityException, IOException;
    static native byte[] publicKey(Object vault, byte[] handle) throws GeneralSecurityException, IOException;
    static native byte[] sign(Object vault, byte[] handle, byte[] publicKey, byte[] transcript) throws GeneralSecurityException, IOException;
    static native void delete(Object vault, byte[] handle) throws GeneralSecurityException, IOException;
}
