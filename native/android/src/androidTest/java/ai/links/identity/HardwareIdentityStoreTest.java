package ai.links.identity;

import android.content.Context;
import androidx.test.platform.app.InstrumentationRegistry;
import androidx.test.ext.junit.runners.AndroidJUnit4;
import java.nio.charset.StandardCharsets;
import java.security.GeneralSecurityException;
import java.security.KeyStore;
import org.junit.Test;
import org.junit.runner.RunWith;
import static org.junit.Assert.*;

/** Physical TEE/StrongBox device with secure screen lock, unlocked for these tests. */
@RunWith(AndroidJUnit4.class)
public class HardwareIdentityStoreTest {
    @Test public void rustSigningRestoreMismatchAndDeletion() throws Exception {
        Context context = InstrumentationRegistry.getInstrumentation().getTargetContext();
        HardwareIdentityStore store = new HardwareIdentityStore(context);
        HardwareIdentityStore.KeyReference first = store.createIdentity();
        HardwareIdentityStore.KeyReference second = null;
        byte[] message = "links/test/v1\0physical".getBytes(StandardCharsets.UTF_8);
        try {
            HardwareIdentityStore reopened = new HardwareIdentityStore(context);
            reopened.validateIdentity(first);
            byte[] signature = store.sign(first, message);
            assertEquals(64, signature.length);
            assertArrayEquals(signature, reopened.sign(first, message));
            second = store.createIdentity();
            HardwareIdentityStore.KeyReference swapped = new HardwareIdentityStore.KeyReference(second.handle(), first.publicKey());
            try { store.sign(swapped, message); fail("Substituted identity signed"); }
            catch (GeneralSecurityException expected) { }
            // Simulate permanent wrapping-key loss while keeping the sealed record.
            KeyStore keystore = KeyStore.getInstance("AndroidKeyStore");
            keystore.load(null);
            keystore.deleteEntry("ai.links.identity.seed.v1." + first.handle());
            try { reopened.sign(first, message); fail("Invalidated key signed/regenerated"); }
            catch (GeneralSecurityException expected) { }
            assertFalse(keystore.containsAlias("ai.links.identity.seed.v1." + first.handle()));
        } finally {
            store.deleteIdentity(first);
            if (second != null) store.deleteIdentity(second);
        }
        try { store.sign(first, message); fail("Deleted identity signed/regenerated"); }
        catch (GeneralSecurityException expected) { }
    }
}
