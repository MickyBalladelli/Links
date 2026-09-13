package ai.links.app;

import android.util.Base64;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.UUID;
import javax.net.ssl.HttpsURLConnection;
import org.json.JSONException;
import org.json.JSONObject;

/** HTTPS client for the authenticated WebAuthn and opaque-backup endpoints. */
public final class PasskeyClient {
    private static final int MAX_BODY_BYTES = 16 * 1024;
    private static final int MAX_CREDENTIAL_BYTES = 1024;
    private static final int MAX_ENVELOPE_BYTES = 1152;
    private static final int BASE64_FLAGS = Base64.URL_SAFE | Base64.NO_WRAP | Base64.NO_PADDING;
    private final String baseUrl;

    public PasskeyClient(String baseUrl) throws IOException {
        if (baseUrl == null || baseUrl.isEmpty()) throw new IOException("Invalid auth endpoint");
        try {
            URL parsed = new URL(baseUrl);
            if (!"https".equalsIgnoreCase(parsed.getProtocol())
                    || parsed.getUserInfo() != null || parsed.getQuery() != null
                    || parsed.getRef() != null || parsed.getHost().isEmpty())
                throw new IOException("Invalid auth endpoint");
            this.baseUrl = baseUrl.endsWith("/")
                    ? baseUrl.substring(0, baseUrl.length() - 1) : baseUrl;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid auth endpoint", error);
        }
    }

    public Options startRegistration(String accessToken) throws IOException {
        return new Options(request("POST", "/v1/passkeys/register/start", accessToken,
                new JSONObject()));
    }

    public RegisteredPasskey finishRegistration(String accessToken, Options options,
            byte[] credentialId, byte[] clientDataJson, byte[] attestationObject) throws IOException {
        requireOptions(options);
        requireBytes(credentialId, 1, MAX_CREDENTIAL_BYTES, "credential ID");
        requireBytes(clientDataJson, 1, 4096, "client data");
        requireBytes(attestationObject, 1, 8192, "attestation");
        JSONObject body = new JSONObject();
        put(body, "challenge_id", options.challengeId);
        put(body, "credential_id", encode(credentialId));
        put(body, "client_data_json", encode(clientDataJson));
        put(body, "attestation_object", encode(attestationObject));
        return new RegisteredPasskey(request("POST", "/v1/passkeys/register/finish",
                accessToken, body));
    }

    public Options startAssertion(String accessToken) throws IOException {
        return new Options(request("POST", "/v1/passkeys/assert/start", accessToken,
                new JSONObject()));
    }

    public Assertion finishAssertion(String accessToken, Options options, byte[] credentialId,
            byte[] clientDataJson, byte[] authenticatorData, byte[] signature) throws IOException {
        requireOptions(options);
        requireBytes(credentialId, 1, MAX_CREDENTIAL_BYTES, "credential ID");
        requireBytes(clientDataJson, 1, 4096, "client data");
        requireBytes(authenticatorData, 1, 4096, "authenticator data");
        requireBytes(signature, 1, 1024, "signature");
        JSONObject body = new JSONObject();
        put(body, "challenge_id", options.challengeId);
        put(body, "credential_id", encode(credentialId));
        put(body, "client_data_json", encode(clientDataJson));
        put(body, "authenticator_data", encode(authenticatorData));
        put(body, "signature", encode(signature));
        return new Assertion(request("POST", "/v1/passkeys/assert/finish", accessToken, body));
    }

    public void putBackup(String accessToken, UUID backupId, UUID deviceId,
            byte[] credentialId, byte[] envelope) throws IOException {
        requireUuid(backupId, "backup ID");
        requireUuid(deviceId, "device ID");
        requireBytes(credentialId, 1, MAX_CREDENTIAL_BYTES, "credential ID");
        requireBytes(envelope, 1, MAX_ENVELOPE_BYTES, "backup envelope");
        JSONObject body = new JSONObject();
        put(body, "backup_id", backupId.toString());
        put(body, "device_id", deviceId.toString());
        put(body, "credential_id", encode(credentialId));
        put(body, "encrypted_envelope", encode(envelope));
        request("PUT", "/v1/passkey-backups", accessToken, body);
    }

    public Backup getBackup(String accessToken, UUID backupId) throws IOException {
        requireUuid(backupId, "backup ID");
        return new Backup(request("GET", "/v1/passkey-backups/" + backupId,
                accessToken, null));
    }

    static String encode(byte[] bytes) {
        return Base64.encodeToString(bytes, BASE64_FLAGS);
    }

    private JSONObject request(String method, String path, String accessToken, JSONObject body)
            throws IOException {
        requireToken(accessToken);
        HttpsURLConnection connection = null;
        try {
            URL endpoint = new URL(baseUrl + path);
            if (!"https".equalsIgnoreCase(endpoint.getProtocol()))
                throw new IOException("Invalid auth endpoint");
            connection = (HttpsURLConnection) endpoint.openConnection();
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(10000);
            connection.setInstanceFollowRedirects(false);
            connection.setRequestMethod(method);
            connection.setRequestProperty("Authorization", "Bearer " + accessToken);
            connection.setRequestProperty("Accept", "application/json");
            if (body != null) {
                byte[] encodedBody = body.toString().getBytes(StandardCharsets.UTF_8);
                if (encodedBody.length > MAX_BODY_BYTES) throw new IOException("Request too large");
                connection.setDoOutput(true);
                connection.setFixedLengthStreamingMode(encodedBody.length);
                connection.setRequestProperty("Content-Type", "application/json; charset=utf-8");
                try (OutputStream output = connection.getOutputStream()) {
                    output.write(encodedBody);
                }
            }
            int responseCode = connection.getResponseCode();
            InputStream responseStream = responseCode >= 400
                    ? connection.getErrorStream() : connection.getInputStream();
            if (responseCode == 204) return null;
            String response = readLimited(responseStream);
            if (responseCode < 200 || responseCode >= 300)
                throw new IOException("Passkey service rejected request");
            try {
                return new JSONObject(response);
            } catch (JSONException error) {
                throw new IOException("Invalid passkey service response", error);
            }
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    private static String readLimited(InputStream input) throws IOException {
        if (input == null) throw new IOException("Empty passkey service response");
        try (InputStream stream = input; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[1024];
            int total = 0;
            int count;
            while ((count = stream.read(buffer)) != -1) {
                total += count;
                if (total > MAX_BODY_BYTES) throw new IOException("Passkey response too large");
                output.write(buffer, 0, count);
            }
            return output.toString(StandardCharsets.UTF_8.name());
        }
    }

    private static void requireToken(String token) throws IOException {
        if (token == null || token.isEmpty() || token.length() > 256
                || token.indexOf('\r') >= 0 || token.indexOf('\n') >= 0)
            throw new IOException("Invalid auth session");
    }

    private static void requireOptions(Options options) throws IOException {
        if (options == null) throw new IOException("Invalid passkey options");
    }

    private static void requireUuid(UUID value, String name) throws IOException {
        if (value == null || value.equals(new UUID(0, 0))) throw new IOException("Invalid " + name);
    }

    private static void requireBytes(byte[] value, int minimum, int maximum, String name)
            throws IOException {
        if (value == null || value.length < minimum || value.length > maximum)
            throw new IOException("Invalid " + name);
    }

    private static void put(JSONObject object, String key, String value) throws IOException {
        try {
            object.put(key, value);
        } catch (JSONException error) {
            throw new IOException("Invalid passkey request", error);
        }
    }

    public static final class Options {
        private final String challengeId;
        private final byte[] challenge;
        private final String rpId;
        private final String userId;
        private final long expiresAtMs;

        private Options(JSONObject response) throws IOException {
            try {
                challengeId = canonicalUuid(response.getString("challenge_id"), "challenge ID");
                challenge = decodeExact(response.getString("challenge"), 32, "challenge");
                rpId = response.getString("rp_id");
                if (rpId.isEmpty() || rpId.length() > 253
                        || rpId.indexOf('\r') >= 0 || rpId.indexOf('\n') >= 0)
                    throw new IOException("Invalid passkey RP ID");
                userId = canonicalUuid(response.getString("user_id"), "user ID");
                expiresAtMs = response.getLong("expires_at_ms");
                if (expiresAtMs <= 0) throw new IOException("Invalid passkey challenge");
            } catch (JSONException error) {
                throw new IOException("Invalid passkey challenge", error);
            }
        }

        public String challengeId() { return challengeId; }
        public byte[] challengeBytes() { return challenge.clone(); }
        public String rpId() { return rpId; }
        public String userId() { return userId; }
        public long expiresAtMs() { return expiresAtMs; }
    }

    public static final class RegisteredPasskey {
        private final byte[] credentialId;
        private final long signCount;

        private RegisteredPasskey(JSONObject response) throws IOException {
            try {
                credentialId = decodeBounded(response.getString("credential_id"),
                        MAX_CREDENTIAL_BYTES, "credential ID");
                signCount = response.getLong("sign_count");
                if (signCount < 0 || signCount > 0xffffffffL) throw new IOException("Invalid sign count");
            } catch (JSONException error) {
                throw new IOException("Invalid passkey registration response", error);
            }
        }

        public byte[] credentialId() { return credentialId.clone(); }
        public long signCount() { return signCount; }
    }

    public static final class Assertion {
        private final byte[] credentialId;
        private final long signCount;

        private Assertion(JSONObject response) throws IOException {
            try {
                credentialId = decodeBounded(response.getString("credential_id"),
                        MAX_CREDENTIAL_BYTES, "credential ID");
                signCount = response.getLong("sign_count");
                if (signCount < 0 || signCount > 0xffffffffL) throw new IOException("Invalid sign count");
            } catch (JSONException error) {
                throw new IOException("Invalid passkey assertion response", error);
            }
        }

        public byte[] credentialId() { return credentialId.clone(); }
        public long signCount() { return signCount; }
    }

    public static final class Backup {
        private final String backupId;
        private final String deviceId;
        private final byte[] credentialId;
        private final byte[] envelope;

        private Backup(JSONObject response) throws IOException {
            try {
                backupId = canonicalUuid(response.getString("backup_id"), "backup ID");
                deviceId = canonicalUuid(response.getString("device_id"), "device ID");
                credentialId = decodeBounded(response.getString("credential_id"),
                        MAX_CREDENTIAL_BYTES, "credential ID");
                envelope = decodeBounded(response.getString("encrypted_envelope"),
                        MAX_ENVELOPE_BYTES, "backup envelope");
            } catch (JSONException error) {
                throw new IOException("Invalid passkey backup response", error);
            }
        }

        public UUID backupId() { return UUID.fromString(backupId); }
        public UUID deviceId() { return UUID.fromString(deviceId); }
        public byte[] credentialId() { return credentialId.clone(); }
        public byte[] envelope() { return envelope.clone(); }
    }

    private static String canonicalUuid(String value, String name) throws IOException {
        try {
            UUID uuid = UUID.fromString(value);
            if (uuid.equals(new UUID(0, 0)) || !uuid.toString().equals(value))
                throw new IllegalArgumentException();
            return value;
        } catch (IllegalArgumentException | NullPointerException error) {
            throw new IOException("Invalid " + name, error);
        }
    }

    private static byte[] decodeExact(String value, int length, String name) throws IOException {
        byte[] decoded = decodeBounded(value, length, name);
        if (decoded.length != length || !encode(decoded).equals(value))
            throw new IOException("Invalid " + name);
        return decoded;
    }

    private static byte[] decodeBounded(String value, int maximum, String name) throws IOException {
        if (value == null) throw new IOException("Invalid " + name);
        try {
            byte[] decoded = Base64.decode(value, BASE64_FLAGS);
            if (decoded.length == 0 || decoded.length > maximum || !encode(decoded).equals(value))
                throw new IllegalArgumentException();
            return decoded;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid " + name, error);
        }
    }
}
