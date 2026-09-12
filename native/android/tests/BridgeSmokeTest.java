package ai.links.identity;

import java.nio.charset.StandardCharsets;
import java.io.IOException;
import java.security.GeneralSecurityException;
import java.security.KeyFactory;
import java.security.Signature;
import java.security.spec.X509EncodedKeySpec;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/** Host JVM fixture tests exercise real JNI + Rust, NOT Android secure hardware. */
public final class BridgeSmokeTest {
    public static final class Vault {
        final Map<String, byte[]> seeds = new HashMap<>();
        byte[] lastStoreInput, lastLoadOutput;
        boolean failStore, failLoad, failDelete, failIO;
        int stores, loads, deletes;
        int returnedSeedSize = 32;
        public String storeSeed(byte[] seed) throws GeneralSecurityException {
            stores++;
            lastStoreInput = seed;
            if (failStore) throw new GeneralSecurityException("fixture store failure");
            String handle = UUID.randomUUID().toString();
            seeds.put(handle, seed.clone());
            return handle;
        }
        public byte[] loadSeed(String handle) throws GeneralSecurityException, IOException {
            loads++;
            if (failIO) throw new IOException("fixture storage failure");
            if (failLoad || !seeds.containsKey(handle)) throw new GeneralSecurityException("fixture load failure");
            lastLoadOutput = Arrays.copyOf(seeds.get(handle), returnedSeedSize);
            return lastLoadOutput;
        }
        public void deleteSeed(String handle) throws GeneralSecurityException {
            deletes++;
            if (failDelete) throw new GeneralSecurityException("fixture delete failure");
            byte[] removed = seeds.remove(handle);
            if (removed != null) Arrays.fill(removed, (byte)0);
        }
    }
    interface Action { void run() throws Exception; }
    static void reject(Action action) throws Exception {
        try { action.run(); } catch (GeneralSecurityException expected) { return; }
        throw new AssertionError("Expected failure");
    }
    static void check(boolean value) { if (!value) throw new AssertionError(); }
    static boolean wiped(byte[] bytes) { return bytes != null && Arrays.equals(bytes, new byte[bytes.length]); }
    public static void main(String[] args) throws Exception {
        Vault vault = new Vault();
        byte[] identity = NativeIdentityBridge.create(vault);
        byte[] handle = Arrays.copyOfRange(identity, 0, 36);
        byte[] publicKey = Arrays.copyOfRange(identity, 36, 68);
        check(wiped(vault.lastStoreInput) && wiped(vault.lastLoadOutput));
        check(Arrays.equals(publicKey, NativeIdentityBridge.publicKey(vault, handle)));
        byte[] transcript = "links/test/v1\0jni".getBytes(StandardCharsets.UTF_8);
        byte[] signature = NativeIdentityBridge.sign(vault, handle, publicKey, transcript);
        byte[] spki = new byte[44];
        byte[] prefix = {0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00};
        System.arraycopy(prefix, 0, spki, 0, 12);
        System.arraycopy(publicKey, 0, spki, 12, 32);
        Signature verifier = Signature.getInstance("Ed25519");
        verifier.initVerify(KeyFactory.getInstance("Ed25519").generatePublic(new X509EncodedKeySpec(spki)));
        verifier.update(transcript);
        check(verifier.verify(signature));
        check(vault.loads == 3 && wiped(vault.lastLoadOutput));
        vault.returnedSeedSize = 31;
        reject(() -> NativeIdentityBridge.sign(vault, handle, publicKey, transcript));
        check(wiped(vault.lastLoadOutput));
        vault.returnedSeedSize = 32;
        vault.failIO = true;
        try {
            NativeIdentityBridge.publicKey(vault, handle);
            throw new AssertionError("Expected storage failure");
        } catch (IOException expected) { check(expected.getMessage().equals("fixture storage failure")); }
        vault.failIO = false;
        reject(() -> NativeIdentityBridge.sign(vault, handle, new byte[32], transcript));
        check(wiped(vault.lastLoadOutput));
        reject(() -> NativeIdentityBridge.sign(vault, handle, publicKey, new byte[1024 * 1024 + 1]));
        reject(() -> NativeIdentityBridge.sign(vault, new byte[35], publicKey, transcript));
        reject(() -> NativeIdentityBridge.sign(vault, handle, publicKey, null));
        reject(() -> NativeIdentityBridge.create(null));
        vault.failLoad = true;
        reject(() -> NativeIdentityBridge.sign(vault, handle, publicKey, transcript));
        check(vault.stores == 1);
        vault.failLoad = false;
        vault.failDelete = true;
        reject(() -> NativeIdentityBridge.delete(vault, handle));
        vault.failDelete = false;
        NativeIdentityBridge.delete(vault, handle);
        NativeIdentityBridge.delete(vault, handle);
        reject(() -> NativeIdentityBridge.publicKey(vault, handle));
        check(vault.seeds.isEmpty());
        vault.failStore = true;
        reject(() -> NativeIdentityBridge.create(vault));
        check(wiped(vault.lastStoreInput));
        vault.failStore = false;
        vault.failLoad = true;
        int deletes = vault.deletes;
        reject(() -> NativeIdentityBridge.create(vault));
        check(vault.deletes == deletes + 1 && vault.seeds.isEmpty());
        check(wiped(vault.lastStoreInput));
        System.out.println("JNI/Rust lifecycle, signature, validation, failure cleanup and managed-buffer wiping passed (fixture vault only).");
    }
}
