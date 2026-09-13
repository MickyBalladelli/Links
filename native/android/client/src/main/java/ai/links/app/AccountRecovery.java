package ai.links.app;

import ai.links.identity.HardwareIdentityStore;
import java.io.IOException;
import java.security.GeneralSecurityException;
import java.security.SecureRandom;
import java.util.Arrays;
import java.util.UUID;
import org.json.JSONException;
import org.json.JSONObject;

/** Explicit Android account recovery coordinator. Secrets stay in this process. */
public final class AccountRecovery {
    private static final SecureRandom RANDOM = new SecureRandom();

    /** Adapter around Android Credential Manager/WebAuthn. It must request UV and PRF. */
    public interface PasskeyProvider {
        Registration create(PasskeyClient.Options options) throws Exception;
        Assertion get(PasskeyClient.Options options, byte[] prfSalt) throws Exception;
    }

    public static final class Registration {
        private final byte[] credentialId;
        private final byte[] clientDataJson;
        private final byte[] attestationObject;

        public Registration(byte[] credentialId, byte[] clientDataJson, byte[] attestationObject)
                throws IOException {
            require(credentialId, 1, 1024, "credential ID");
            require(clientDataJson, 1, 4096, "client data");
            require(attestationObject, 1, 8192, "attestation");
            this.credentialId = credentialId.clone();
            this.clientDataJson = clientDataJson.clone();
            this.attestationObject = attestationObject.clone();
        }

        byte[] credentialId() { return credentialId.clone(); }
        byte[] clientDataJson() { return clientDataJson.clone(); }
        byte[] attestationObject() { return attestationObject.clone(); }
    }

    public static final class Assertion {
        private final byte[] credentialId;
        private final byte[] clientDataJson;
        private final byte[] authenticatorData;
        private final byte[] signature;
        private final byte[] prfOutput;

        public Assertion(byte[] credentialId, byte[] clientDataJson, byte[] authenticatorData,
                byte[] signature, byte[] prfOutput) throws IOException {
            require(credentialId, 1, 1024, "credential ID");
            require(clientDataJson, 1, 4096, "client data");
            require(authenticatorData, 1, 4096, "authenticator data");
            require(signature, 1, 1024, "signature");
            require(prfOutput, 32, 32, "passkey PRF output");
            if (allZero(prfOutput)) throw new IOException("Invalid passkey PRF output");
            this.credentialId = credentialId.clone();
            this.clientDataJson = clientDataJson.clone();
            this.authenticatorData = authenticatorData.clone();
            this.signature = signature.clone();
            this.prfOutput = prfOutput.clone();
        }

        byte[] credentialId() { return credentialId.clone(); }
        byte[] clientDataJson() { return clientDataJson.clone(); }
        byte[] authenticatorData() { return authenticatorData.clone(); }
        byte[] signature() { return signature.clone(); }
        byte[] prfOutput() { return prfOutput.clone(); }
    }

    private AccountRecovery() {}

    public static PasskeyClient.RegisteredPasskey registerPasskey(ClientSession session,
            PasskeyClient api, PasskeyProvider provider) throws Exception {
        requireSession(session);
        if (api == null || provider == null) throw new IOException("Passkey flow unavailable");
        String token = session.accessToken();
        PasskeyClient.Options options = api.startRegistration(token);
        Registration response = provider.create(options);
        return api.finishRegistration(token, options, response.credentialId(),
                response.clientDataJson(), response.attestationObject());
    }

    /** Create and upload one opaque identity backup. */
    public static void backupWithPasskey(ClientSession session, PasskeyClient api,
            PasskeyProvider provider, UUID backupId) throws Exception {
        requireSession(session);
        if (api == null || provider == null || backupId == null || backupId.equals(new UUID(0, 0)))
            throw new IOException("Passkey backup unavailable");
        String token = session.accessToken();
        byte[] salt = new byte[32];
        RANDOM.nextBytes(salt);
        Assertion response = null;
        byte[] prf = null;
        byte[] credentialId = null;
        byte[] envelope = null;
        try {
            PasskeyClient.Options options = api.startAssertion(token);
            response = provider.get(options, salt);
            credentialId = response.credentialId();
            PasskeyClient.Assertion verified = api.finishAssertion(token, options, credentialId,
                    response.clientDataJson(), response.authenticatorData(), response.signature());
            if (!Arrays.equals(credentialId, verified.credentialId()))
                throw new GeneralSecurityException("Passkey credential mismatch");
            prf = response.prfOutput();
            envelope = session.createPasskeyBackup(backupId, credentialId, salt, prf);
            api.putBackup(token, backupId, UUID.fromString(session.deviceId()), credentialId, envelope);
        } finally {
            wipe(salt);
            wipe(prf);
            wipe(credentialId);
            wipe(envelope);
        }
    }

    /**
     * Restore an opaque backup with an already authenticated bootstrap session.
     * The server token is only transport authorization; it never decrypts the backup.
     */
    public static HardwareIdentityStore.KeyReference restoreFromPasskey(ClientSession target,
            PasskeyClient api, PasskeyProvider provider, String accessToken, UUID backupId)
            throws Exception {
        if (target == null || api == null || provider == null || accessToken == null
                || backupId == null || backupId.equals(new UUID(0, 0)))
            throw new IOException("Passkey restore unavailable");
        PasskeyClient.Backup backup = api.getBackup(accessToken, backupId);
        byte[] envelope = backup.envelope();
        byte[] credentialId = backup.credentialId();
        byte[] salt = envelopeSalt(envelope, backupId, backup.deviceId(), credentialId);
        byte[] prf = null;
        try {
            PasskeyClient.Options options = api.startAssertion(accessToken);
            Assertion response = provider.get(options, salt);
            if (!Arrays.equals(credentialId, response.credentialId()))
                throw new GeneralSecurityException("Passkey credential mismatch");
            PasskeyClient.Assertion verified = api.finishAssertion(accessToken, options,
                    credentialId, response.clientDataJson(), response.authenticatorData(), response.signature());
            if (!Arrays.equals(credentialId, verified.credentialId()))
                throw new GeneralSecurityException("Passkey credential mismatch");
            prf = response.prfOutput();
            return target.restoreFromPasskey(backupId, backup.deviceId(), credentialId, envelope, prf);
        } finally {
            wipe(salt);
            wipe(prf);
            wipe(envelope);
            wipe(credentialId);
        }
    }

    private static void requireSession(ClientSession session) throws IOException {
        if (session == null || !session.isEnrolled() || !session.isAuthenticated())
            throw new IOException("Authenticated identity required");
    }

    private static byte[] envelopeSalt(byte[] envelope, UUID backupId, UUID deviceId,
            byte[] credentialId) throws IOException {
        if (envelope == null || envelope.length < 128 || envelope[0] != 1 || envelope[1] != 1)
            throw new IOException("Invalid passkey backup envelope");
        UUID embeddedBackup = uuid(envelope, 2);
        UUID embeddedDevice = uuid(envelope, 18);
        int credentialLength = ((envelope[34] & 0xff) << 8) | (envelope[35] & 0xff);
        int saltOffset = 36 + credentialLength;
        if (!backupId.equals(embeddedBackup) || !deviceId.equals(embeddedDevice)
                || credentialLength != credentialId.length || saltOffset + 32 + 60 != envelope.length
                || !Arrays.equals(credentialId, Arrays.copyOfRange(envelope, 36, saltOffset)))
            throw new IOException("Invalid passkey backup binding");
        return Arrays.copyOfRange(envelope, saltOffset, saltOffset + 32);
    }

    private static UUID uuid(byte[] bytes, int offset) throws IOException {
        if (offset < 0 || offset + 16 > bytes.length) throw new IOException("Invalid UUID");
        long high = 0;
        long low = 0;
        for (int i = 0; i < 8; i++) high = (high << 8) | (bytes[offset + i] & 0xffL);
        for (int i = 8; i < 16; i++) low = (low << 8) | (bytes[offset + i] & 0xffL);
        return new UUID(high, low);
    }

    private static void require(byte[] bytes, int minimum, int maximum, String name) throws IOException {
        if (bytes == null || bytes.length < minimum || bytes.length > maximum)
            throw new IOException("Invalid " + name);
    }

    private static boolean allZero(byte[] bytes) {
        for (byte value : bytes) if (value != 0) return false;
        return true;
    }

    private static void wipe(byte[] bytes) {
        if (bytes != null) Arrays.fill(bytes, (byte) 0);
    }
}
