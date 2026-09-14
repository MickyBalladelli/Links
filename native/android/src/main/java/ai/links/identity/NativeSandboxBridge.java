package ai.links.identity;

import java.security.GeneralSecurityException;

/** Opaque JNI bridge for the shared native WASM mini-app runtime. */
public final class NativeSandboxBridge {
    static { System.loadLibrary("links_identity_jni"); }

    private NativeSandboxBridge() {}

    public static native long create(byte[] wasm) throws GeneralSecurityException;
    public static native byte[] run(long runtime, byte[] input) throws GeneralSecurityException;
    public static native void destroy(long runtime) throws GeneralSecurityException;
}
