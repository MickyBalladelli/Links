package ai.links.app;

import ai.links.identity.NativeSandboxBridge;
import java.security.GeneralSecurityException;

/** Android facade for the shared native WASM mini-app runtime. */
public final class AndroidMiniAppSandbox implements AutoCloseable {
    private long runtime;

    public AndroidMiniAppSandbox(byte[] module) throws GeneralSecurityException {
        if (module == null || module.length == 0) throw new GeneralSecurityException("Invalid WASM module");
        runtime = NativeSandboxBridge.create(module.clone());
        if (runtime == 0) throw new GeneralSecurityException("Invalid WASM module");
    }

    public synchronized byte[] run(byte[] input) throws GeneralSecurityException {
        if (runtime == 0) throw new GeneralSecurityException("Sandbox is closed");
        if (input == null) throw new GeneralSecurityException("Invalid sandbox input");
        return NativeSandboxBridge.run(runtime, input.clone());
    }

    @Override
    public synchronized void close() throws GeneralSecurityException {
        if (runtime == 0) return;
        long active = runtime;
        runtime = 0;
        NativeSandboxBridge.destroy(active);
    }
}
