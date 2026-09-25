package ai.links.app;

import android.util.Base64;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.Locale;
import java.util.UUID;
import javax.net.ssl.HttpsURLConnection;
import org.json.JSONException;
import org.json.JSONObject;

/** Small HTTPS client for the account-auth OTP endpoints. */
public final class OtpClient {
    private static final int MAX_BODY_BYTES = 16 * 1024;
    private static final int MAX_CREDENTIAL_BYTES = 1024;
    private static final int BASE64_FLAGS = Base64.URL_SAFE | Base64.NO_WRAP | Base64.NO_PADDING;
    private final String baseUrl;

    public OtpClient(String baseUrl) throws IOException {
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

    public Challenge start(String phone, String channel, String deviceId, String mlsNodeId,
            String publicKey, String signature) throws IOException {
        validatePhone(phone);
        if (!"sms".equals(channel) && !"whatsapp".equals(channel))
            throw new IOException("Invalid OTP channel");
        canonicalUuid(deviceId);
        canonicalUuid(mlsNodeId);
        decodeExact(publicKey, 32);
        decodeExact(signature, 64);
        JSONObject body = new JSONObject();
        put(body, "phone", phone);
        put(body, "channel", channel);
        put(body, "device_id", deviceId);
        put(body, "mls_node_id", mlsNodeId);
        put(body, "public_key", publicKey);
        put(body, "signature", signature);
        return new Challenge(post("/v1/auth/start", body));
    }

    public Session finish(String challengeId, String code, String signature) throws IOException {
        canonicalUuid(challengeId);
        if (code == null || code.length() < 6 || code.length() > 10) {
            throw new IOException("Invalid OTP code");
        }
        for (int i = 0; i < code.length(); i++) {
            if (code.charAt(i) < '0' || code.charAt(i) > '9')
                throw new IOException("Invalid OTP code");
        }
        decodeExact(signature, 64);
        JSONObject body = new JSONObject();
        put(body, "challenge_id", challengeId);
        put(body, "code", code);
        put(body, "signature", signature);
        return new Session(post("/v1/auth/finish", body));
    }

    public String changeUsername(String accessToken, String handle) throws IOException {
        decodeExact(accessToken, 32);
        String cleanHandle = handle == null ? "" : handle.trim()
                .replaceFirst("^@", "").toLowerCase(Locale.ROOT);
        if (!cleanHandle.matches("[a-z][a-z0-9_]{2,31}"))
            throw new IOException("Use a lowercase username with 3–32 letters, numbers, or underscores");
        JSONObject body = new JSONObject();
        put(body, "handle", cleanHandle);
        JSONObject response = request("PUT", "/v1/account/username", body, accessToken);
        try {
            String updatedHandle = response.getString("handle");
            if (!cleanHandle.equals(updatedHandle))
                throw new IOException("Invalid username response");
            return updatedHandle;
        } catch (JSONException error) {
            throw new IOException("Invalid username response", error);
        }
    }

    public String currentUsername(String accessToken) throws IOException {
        decodeExact(accessToken, 32);
        JSONObject response = request("GET", "/v1/account/username", null, accessToken);
        try {
            if (!response.has("handle")) throw new IOException("Invalid username response");
            if (response.isNull("handle")) return "";
            String handle = response.getString("handle");
            if (!handle.matches("[a-z][a-z0-9_]{2,31}"))
                throw new IOException("Invalid username response");
            return handle;
        } catch (JSONException error) {
            throw new IOException("Invalid username response", error);
        }
    }

    /** Revoke the current bearer. Local callers must clear their session even if this fails. */
    public void logout(String accessToken) throws IOException {
        decodeExact(accessToken, 32);
        HttpsURLConnection connection = null;
        try {
            URL endpoint = new URL(baseUrl + "/v1/auth/logout");
            connection = (HttpsURLConnection) endpoint.openConnection();
            connection.setConnectTimeout(5000);
            connection.setReadTimeout(10000);
            connection.setInstanceFollowRedirects(false);
            connection.setRequestMethod("POST");
            connection.setRequestProperty("Authorization", "Bearer " + accessToken);
            connection.setFixedLengthStreamingMode(0);
            connection.setDoOutput(true);
            connection.getOutputStream().close();
            int responseCode = connection.getResponseCode();
            readLimited(responseCode >= 400 ? connection.getErrorStream() : connection.getInputStream());
            if (responseCode < 200 || responseCode >= 300)
                throw new IOException("Logout service rejected request");
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    static String encode(byte[] bytes) {
        return Base64.encodeToString(bytes, BASE64_FLAGS);
    }

    private JSONObject post(String path, JSONObject body) throws IOException {
        return request("POST", path, body, null);
    }

    private JSONObject request(String method, String path, JSONObject body, String accessToken)
            throws IOException {
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
            connection.setRequestProperty("Accept", "application/json");
            if (accessToken != null)
                connection.setRequestProperty("Authorization", "Bearer " + accessToken);
            if (body != null) {
                byte[] encodedBody = body.toString().getBytes(StandardCharsets.UTF_8);
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
            String response = readLimited(responseStream);
            if (responseCode < 200 || responseCode >= 300) {
                if ("/v1/account/username".equals(path)) {
                    if (responseCode == 400)
                        throw new IOException("Use a valid lowercase username");
                    if (responseCode == 401)
                        throw new IOException("Sign in again to manage this username");
                    if (responseCode == 409)
                        throw new IOException("That username is already in use");
                }
                throw new IOException("OTP service rejected request");
            }
            try {
                return new JSONObject(response);
            } catch (JSONException error) {
                throw new IOException("Invalid OTP service response", error);
            }
        } finally {
            if (connection != null) connection.disconnect();
        }
    }

    private static String readLimited(InputStream input) throws IOException {
        if (input == null) throw new IOException("Empty OTP service response");
        try (InputStream stream = input; ByteArrayOutputStream output = new ByteArrayOutputStream()) {
            byte[] buffer = new byte[1024];
            int total = 0;
            int count;
            while ((count = stream.read(buffer)) != -1) {
                total += count;
                if (total > MAX_BODY_BYTES) throw new IOException("OTP response too large");
                output.write(buffer, 0, count);
            }
            return output.toString(StandardCharsets.UTF_8.name());
        }
    }

    private static void validatePhone(String phone) throws IOException {
        if (phone == null || phone.length() < 9 || phone.length() > 16
                || !phone.startsWith("+") || phone.charAt(1) == '0')
            throw new IOException("Invalid phone number");
        for (int i = 1; i < phone.length(); i++) {
            if (phone.charAt(i) < '0' || phone.charAt(i) > '9')
                throw new IOException("Invalid phone number");
        }
    }

    private static String canonicalUuid(String value) throws IOException {
        if (value == null) throw new IOException("Invalid OTP identity binding");
        try {
            UUID uuid = UUID.fromString(value);
            if (!uuid.toString().equals(value)
                    || (uuid.getMostSignificantBits() == 0 && uuid.getLeastSignificantBits() == 0))
                throw new IllegalArgumentException();
            return value;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid OTP identity binding", error);
        }
    }

    private static byte[] decodeExact(String value, int length) throws IOException {
        if (value == null) throw new IOException("Invalid encoded OTP field");
        try {
            byte[] decoded = Base64.decode(value, BASE64_FLAGS);
            if (decoded.length != length || !encode(decoded).equals(value))
                throw new IllegalArgumentException();
            return decoded;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid encoded OTP field", error);
        }
    }

    private static byte[] decodeBounded(String value, int maximum) throws IOException {
        if (value == null) throw new IOException("Invalid encoded OTP field");
        try {
            byte[] decoded = Base64.decode(value, BASE64_FLAGS);
            if (decoded.length == 0 || decoded.length > maximum || !encode(decoded).equals(value))
                throw new IllegalArgumentException();
            return decoded;
        } catch (IllegalArgumentException error) {
            throw new IOException("Invalid encoded OTP field", error);
        }
    }

    private static void put(JSONObject object, String key, String value) throws IOException {
        try {
            object.put(key, value);
        } catch (JSONException error) {
            throw new IOException("Invalid OTP request", error);
        }
    }

    public static final class Challenge {
        private final String challengeId;
        private final String userId;
        private final String deviceId;
        private final String mlsNodeId;
        private final byte[] publicKey;
        private final byte[] nonce;
        private final long expiresAtMs;
        private final byte[] mlsCredential;

        private Challenge(JSONObject response) throws IOException {
            try {
                challengeId = canonicalUuid(response.getString("challenge_id"));
                userId = canonicalUuid(response.getString("user_id"));
                deviceId = canonicalUuid(response.getString("device_id"));
                mlsNodeId = canonicalUuid(response.getString("mls_node_id"));
                publicKey = decodeExact(response.getString("public_key"), 32);
                nonce = decodeExact(response.getString("nonce"), 32);
                expiresAtMs = response.getLong("expires_at_ms");
                if (expiresAtMs <= 0) throw new IOException("Invalid OTP challenge");
                mlsCredential = decodeBounded(response.getString("mls_credential"), MAX_CREDENTIAL_BYTES);
            } catch (JSONException error) {
                throw new IOException("Invalid OTP challenge", error);
            }
        }

        public String challengeId() { return challengeId; }
        public String userId() { return userId; }
        public String deviceId() { return deviceId; }
        public String mlsNodeId() { return mlsNodeId; }
        public byte[] publicKeyBytes() { return publicKey.clone(); }
        public byte[] nonceBytes() { return nonce.clone(); }
        public long expiresAtMs() { return expiresAtMs; }
        public byte[] mlsCredentialBytes() { return mlsCredential.clone(); }
    }

    public static final class Session {
        private final String accessToken;
        private final long expiresAtMs;
        private final String userId;
        private final String deviceId;

        private Session(JSONObject response) throws IOException {
            try {
                String token = response.getString("access_token");
                decodeExact(token, 32);
                accessToken = token;
                expiresAtMs = response.getLong("expires_at_ms");
                if (expiresAtMs <= 0) throw new IOException("Invalid OTP session");
                userId = canonicalUuid(response.getString("user_id"));
                deviceId = canonicalUuid(response.getString("device_id"));
            } catch (JSONException error) {
                throw new IOException("Invalid OTP session", error);
            }
        }

        public String accessToken() { return accessToken; }
        public long expiresAtMs() { return expiresAtMs; }
        public String userId() { return userId; }
        public String deviceId() { return deviceId; }
    }
}
