package ai.links.identity;

import android.content.Context;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import java.io.File;
import java.io.RandomAccessFile;
import java.util.Arrays;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

/** Must run on an unlocked, hardware-backed physical Android device.
 * An emulator/software Keystore is expected to fail; do not waive this gate. */
@RunWith(AndroidJUnit4.class)
public class HardwareSeedVaultTest {
    @Test public void roundtripRestartTamperAndDelete() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        HardwareSeedVault vault = new HardwareSeedVault(context);
        byte[] seed = new byte[32]; Arrays.fill(seed, (byte)7);
        String handle = vault.storeSeed(seed);
        try {
            byte[] restored = new HardwareSeedVault(context).loadSeed(handle);
            try { assertArrayEquals(seed, restored); } finally { Arrays.fill(restored, (byte)0); }
            File record = new File(context.getNoBackupFilesDir(), "links-identity/" + handle + ".sealed");
            try (RandomAccessFile file = new RandomAccessFile(record, "rw")) { file.seek(20); int original = file.read(); file.seek(20); file.write(original ^ 1); }
            try { vault.loadSeed(handle); fail("Tampered ciphertext accepted"); } catch (java.security.GeneralSecurityException expected) { }
        } finally { Arrays.fill(seed, (byte)0); vault.deleteSeed(handle); }
        try { vault.loadSeed(handle); fail("Deleted identity was regenerated"); } catch (java.security.GeneralSecurityException expected) { }
    }
    @Test public void invalidInputIsRejected() throws Exception {
        HardwareSeedVault vault = new HardwareSeedVault(InstrumentationRegistry.getInstrumentation().getTargetContext());
        try { vault.storeSeed(new byte[31]); fail("Bad seed length accepted"); } catch (java.security.GeneralSecurityException expected) { }
        try { vault.deleteSeed("../other"); fail("Bad handle accepted"); } catch (java.security.GeneralSecurityException expected) { }
    }
}
