package ai.links.identity;

import android.content.Context;
import android.os.Build;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyInfo;
import android.security.keystore.KeyProperties;
import android.util.AtomicFile;
import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.KeyStore;
import java.util.Arrays;
import java.util.UUID;
import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.GCMParameterSpec;

/** Hardware-wrapped Ed25519 seed storage. Signing occurs in app memory, not TEE.
 * No software-key fallback. Records live in noBackupFilesDir, not cloud backup.
 * Serialize vault operations through one client identity worker. */
public final class HardwareSeedVault {
    private static final String PREFIX = "ai.links.identity.seed.v1.";
    private static final byte[] MAGIC = new byte[] {'L','K','S','1'};
    private final File directory;
    private final KeyStore keyStore;

    public HardwareSeedVault(Context context) throws GeneralSecurityException, IOException {
        directory = new File(context.getNoBackupFilesDir(), "links-identity");
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Identity storage unavailable");
        keyStore = KeyStore.getInstance("AndroidKeyStore");
        keyStore.load(null);
    }
    public synchronized String storeSeed(byte[] seed) throws GeneralSecurityException, IOException {
        if (seed == null || seed.length != 32) throw new GeneralSecurityException("Invalid identity seed");
        String handle = UUID.randomUUID().toString();
        String alias = alias(handle);
        try {
            KeyGenerator generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore");
            generator.init(new KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                    .setKeySize(256).setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true).setUnlockedDeviceRequired(true).build());
            SecretKey key = generator.generateKey();
            requireHardware(key);
            Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
            cipher.init(Cipher.ENCRYPT_MODE, key); // Keystore generates a fresh IV.
            cipher.updateAAD(alias.getBytes(StandardCharsets.UTF_8));
            byte[] ciphertext = cipher.doFinal(seed);
            byte[] nonce = cipher.getIV();
            if (nonce.length != 12) throw new GeneralSecurityException("Unexpected nonce size");
            byte[] record = ByteBuffer.allocate(4 + 12 + ciphertext.length).put(MAGIC).put(nonce).put(ciphertext).array();
            AtomicFile file = record(handle);
            FileOutputStream output = null;
            try { output = file.startWrite(); output.write(record); file.finishWrite(output); }
            catch (IOException error) { if (output != null) file.failWrite(output); throw error; }
            return handle;
        } catch (GeneralSecurityException | IOException | RuntimeException error) {
            try { keyStore.deleteEntry(alias); } catch (GeneralSecurityException cleanup) { error.addSuppressed(cleanup); }
            record(handle).delete();
            throw error;
        }
    }
    /** Caller wipes the returned buffer after handing it to the Rust signer. */
    public synchronized byte[] loadSeed(String handle) throws GeneralSecurityException, IOException {
        String alias = alias(handle);
        // Reject unexpected file sizes before allocating/reading untrusted bytes.
        AtomicFile file = record(handle);
        long size = file.getBaseFile().length();
        if (size != 64) throw new GeneralSecurityException("Identity record unavailable");
        byte[] record = file.readFully();
        if (record.length != 64 || !Arrays.equals(Arrays.copyOfRange(record, 0, 4), MAGIC))
            throw new GeneralSecurityException("Invalid identity record");
        java.security.Key stored = keyStore.getKey(alias, null);
        if (!(stored instanceof SecretKey)) throw new GeneralSecurityException("Hardware key unavailable");
        SecretKey key = (SecretKey) stored;
        requireHardware(key);
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.DECRYPT_MODE, key, new GCMParameterSpec(128, Arrays.copyOfRange(record, 4, 16)));
        cipher.updateAAD(alias.getBytes(StandardCharsets.UTF_8));
        byte[] seed = cipher.doFinal(record, 16, record.length - 16);
        if (seed.length != 32) { Arrays.fill(seed, (byte)0); throw new GeneralSecurityException("Invalid identity seed"); }
        return seed;
    }
    public synchronized void deleteSeed(String handle) throws GeneralSecurityException {
        keyStore.deleteEntry(alias(handle));
        record(handle).delete();
    }
    @SuppressWarnings("deprecation")
    private void requireHardware(SecretKey key) throws GeneralSecurityException {
        KeyInfo info = (KeyInfo) SecretKeyFactory.getInstance(key.getAlgorithm(), "AndroidKeyStore").getKeySpec(key, KeyInfo.class);
        if (Build.VERSION.SDK_INT >= 31) {
            int level = info.getSecurityLevel();
            if (level != KeyProperties.SECURITY_LEVEL_TRUSTED_ENVIRONMENT && level != KeyProperties.SECURITY_LEVEL_STRONGBOX)
                throw new GeneralSecurityException("Hardware-backed Keystore required");
        } else if (!info.isInsideSecureHardware()) throw new GeneralSecurityException("Hardware-backed Keystore required");
    }
    private static String alias(String handle) throws GeneralSecurityException {
        try { if (!UUID.fromString(handle).toString().equals(handle)) throw new IllegalArgumentException(); }
        catch (IllegalArgumentException | NullPointerException error) { throw new GeneralSecurityException("Invalid key handle"); }
        return PREFIX + handle;
    }
    private AtomicFile record(String handle) throws GeneralSecurityException {
        alias(handle); return new AtomicFile(new File(directory, handle + ".sealed"));
    }
}
