package ai.links.app;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;

/**
 * Android host contract for decentralized Links routes. The transport and
 * storage adapters own their network stacks; this class owns retry order,
 * ciphertext CID checks, and SFrame-only relay selection.
 */
public final class AndroidDecentralizedClient {
    public static final int MAX_ENDPOINTS = 8;
    public static final int MAX_RELAYS = 16;
    public static final int MAX_ENVELOPE_BYTES = 256 * 1024;
    public static final int MAX_CHUNK_BYTES = 256 * 1024;
    public static final long MAX_LEASE_MS = 10 * 60 * 1000;

    public enum RelayMode { OPEN, TOKEN_INCENTIVIZED }

    public static final class MediaRelay {
        public final String nodeId;
        public final String region;
        public final String endpoint;
        public final String turnUrl;
        public final RelayMode mode;
        public final long priceUnitsPerMinute;
        public final int maxBitrateKbps;
        public final long expiresAtMs;
        public final boolean supportsSFrame;
        public final boolean verified;

        public MediaRelay(String nodeId, String region, String endpoint, String turnUrl,
                RelayMode mode, long priceUnitsPerMinute, int maxBitrateKbps,
                long expiresAtMs, boolean supportsSFrame, boolean verified) throws IOException {
            if (!validLocator(nodeId) || !validLocator(region) || !validEndpoint(endpoint, "wss://")
                    || (turnUrl != null && (!turnUrl.startsWith("turn:")
                    && !turnUrl.startsWith("turns:") || !validEndpoint(turnUrl, "turn")))
                    || mode == null || maxBitrateKbps <= 0 || expiresAtMs <= 0
                    || !supportsSFrame || !verified
                    || (mode == RelayMode.OPEN && priceUnitsPerMinute != 0)
                    || (mode == RelayMode.TOKEN_INCENTIVIZED && priceUnitsPerMinute <= 0))
                throw new IOException("Invalid decentralized media relay");
            this.nodeId = nodeId;
            this.region = region;
            this.endpoint = endpoint;
            this.turnUrl = turnUrl;
            this.mode = mode;
            this.priceUnitsPerMinute = priceUnitsPerMinute;
            this.maxBitrateKbps = maxBitrateKbps;
            this.expiresAtMs = expiresAtMs;
            this.supportsSFrame = supportsSFrame;
            this.verified = verified;
        }
    }

    public static final class RelayAccess {
        public final byte[] token;
        public final String relayNodeId;
        public final String sessionId;
        public final long expiresAtMs;
        public final long maxDurationMs;

        public RelayAccess(byte[] token, String relayNodeId, String sessionId,
                long expiresAtMs, long maxDurationMs) throws IOException {
            if (token == null || token.length == 0 || !validLocator(relayNodeId)
                    || !validUuid(sessionId) || expiresAtMs <= 0 || maxDurationMs <= 0)
                throw new IOException("Invalid decentralized relay token");
            this.token = token.clone();
            this.relayNodeId = relayNodeId;
            this.sessionId = sessionId;
            this.expiresAtMs = expiresAtMs;
            this.maxDurationMs = maxDurationMs;
        }
    }

    public static final class MediaRoute {
        public final MediaRelay relay;
        public final String sessionId;
        public final long durationMs;
        private final byte[] accessToken;

        private MediaRoute(MediaRelay relay, String sessionId, long durationMs,
                byte[] accessToken) {
            this.relay = relay;
            this.sessionId = sessionId;
            this.durationMs = durationMs;
            this.accessToken = accessToken == null ? null : accessToken.clone();
        }

        public byte[] copyAccessToken() {
            return accessToken == null ? null : accessToken.clone();
        }
    }

    public static final class Plan {
        public final String region;
        public final List<String> transportEndpoints;
        public final List<String> storageGateways;
        public final List<MediaRelay> mediaRelays;

        public Plan(String region, List<String> transportEndpoints,
                List<String> storageGateways, List<MediaRelay> mediaRelays) throws IOException {
            if (!validLocator(region) || transportEndpoints == null
                    || transportEndpoints.isEmpty() || transportEndpoints.size() > MAX_ENDPOINTS
                    || storageGateways == null || storageGateways.isEmpty()
                    || storageGateways.size() > MAX_ENDPOINTS || mediaRelays == null
                    || mediaRelays.size() > MAX_RELAYS)
                throw new IOException("Invalid decentralized route");
            for (String endpoint : transportEndpoints) {
                if (!validEndpoint(endpoint, "wss://")) throw new IOException("Invalid transport endpoint");
            }
            for (String gateway : storageGateways) {
                if (!validEndpoint(gateway, "https://")) throw new IOException("Invalid storage gateway");
            }
            for (MediaRelay relay : mediaRelays) {
                if (relay == null || !region.equals(relay.region))
                    throw new IOException("Invalid media relay region");
            }
            this.region = region;
            this.transportEndpoints = Collections.unmodifiableList(new ArrayList<>(transportEndpoints));
            this.storageGateways = Collections.unmodifiableList(new ArrayList<>(storageGateways));
            this.mediaRelays = Collections.unmodifiableList(new ArrayList<>(mediaRelays));
        }
    }

    public interface TransportAdapter {
        CompletableFuture<Void> publishOpaque(String endpoint, byte[] envelope);
        CompletableFuture<List<byte[]>> replayOpaque(String endpoint, long afterCursor, int limit);
    }

    public interface ChunkStore {
        CompletableFuture<Void> uploadCiphertext(String gateway, String cid, byte[] ciphertext);
        CompletableFuture<byte[]> downloadCiphertext(String gateway, String cid);
    }

    private static final class IntegrityFailure extends IOException {
        IntegrityFailure() { super("Ciphertext CID mismatch"); }
    }

    private final Plan plan;
    private final TransportAdapter transport;
    private final ChunkStore storage;

    public AndroidDecentralizedClient(Plan plan, TransportAdapter transport, ChunkStore storage)
            throws IOException {
        if (plan == null || transport == null || storage == null)
            throw new IOException("Invalid decentralized adapters");
        this.plan = plan;
        this.transport = transport;
        this.storage = storage;
    }

    public CompletableFuture<Void> publishOpaqueEnvelope(byte[] envelope) {
        if (envelope == null || envelope.length == 0 || envelope.length > MAX_ENVELOPE_BYTES)
            return failed(new IOException("Invalid opaque envelope"));
        return publishAttempt(envelope, 0);
    }

    private CompletableFuture<Void> publishAttempt(byte[] envelope, int index) {
        if (index >= plan.transportEndpoints.size())
            return failed(new IOException("Decentralized transport unavailable"));
        try {
            return transport.publishOpaque(plan.transportEndpoints.get(index), envelope.clone())
                    .exceptionallyCompose(error -> publishAttempt(envelope, index + 1));
        } catch (RuntimeException error) {
            return publishAttempt(envelope, index + 1);
        }
    }

    public CompletableFuture<List<byte[]>> replayOpaque(long afterCursor, int limit) {
        if (afterCursor < 0 || limit < 1 || limit > 100)
            return failed(new IOException("Invalid opaque replay"));
        return replayAttempt(afterCursor, limit, 0);
    }

    private CompletableFuture<List<byte[]>> replayAttempt(long afterCursor, int limit, int index) {
        if (index >= plan.transportEndpoints.size())
            return failed(new IOException("Decentralized transport unavailable"));
        try {
            return transport.replayOpaque(plan.transportEndpoints.get(index), afterCursor, limit)
                    .thenCompose(frames -> {
                        if (frames == null || frames.size() > limit) {
                            return failed(new IOException("Invalid opaque replay"));
                        }
                        for (byte[] frame : frames) {
                            if (frame == null || frame.length == 0 || frame.length > MAX_ENVELOPE_BYTES)
                                return failed(new IOException("Invalid opaque replay"));
                        }
                        return CompletableFuture.completedFuture(copyFrames(frames));
                    })
                    .exceptionallyCompose(error -> replayAttempt(afterCursor, limit, index + 1));
        } catch (RuntimeException error) {
            return replayAttempt(afterCursor, limit, index + 1);
        }
    }

    public CompletableFuture<Void> uploadCiphertextChunk(String cid, byte[] ciphertext) {
        try {
            if (!cid.equals(cidForCiphertext(ciphertext)))
                return failed(new IntegrityFailure());
        } catch (IOException error) {
            return failed(error);
        }
        return uploadAttempt(cid, ciphertext, 0);
    }

    private CompletableFuture<Void> uploadAttempt(String cid, byte[] ciphertext, int index) {
        if (index >= plan.storageGateways.size())
            return failed(new IOException("Decentralized storage unavailable"));
        try {
            return storage.uploadCiphertext(plan.storageGateways.get(index), cid, ciphertext.clone())
                    .exceptionallyCompose(error -> uploadAttempt(cid, ciphertext, index + 1));
        } catch (RuntimeException error) {
            return uploadAttempt(cid, ciphertext, index + 1);
        }
    }

    public CompletableFuture<byte[]> downloadCiphertextChunk(String cid) {
        if (!validCid(cid)) return failed(new IntegrityFailure());
        return downloadAttempt(cid, 0);
    }

    private CompletableFuture<byte[]> downloadAttempt(String cid, int index) {
        if (index >= plan.storageGateways.size())
            return failed(new IOException("Decentralized storage unavailable"));
        try {
            return storage.downloadCiphertext(plan.storageGateways.get(index), cid)
                    .thenCompose(ciphertext -> {
                        try {
                            if (!cid.equals(cidForCiphertext(ciphertext)))
                                return failed(new IntegrityFailure());
                            return CompletableFuture.completedFuture(ciphertext.clone());
                        } catch (IOException error) {
                            return failed(error);
                        }
                    })
                    .exceptionallyCompose(error -> {
                        if (unwrap(error) instanceof IntegrityFailure)
                            return failed(error);
                        return downloadAttempt(cid, index + 1);
                    });
        } catch (RuntimeException error) {
            return downloadAttempt(cid, index + 1);
        }
    }

    public MediaRoute selectMediaRelay(String sessionId, long durationMs,
            RelayAccess access, long nowMs) throws IOException {
        if (!validUuid(sessionId) || durationMs <= 0 || durationMs > MAX_LEASE_MS)
            throw new IOException("Invalid media relay duration");
        for (MediaRelay relay : plan.mediaRelays) {
            if (relay.mode == RelayMode.OPEN && relay.expiresAtMs > nowMs)
                return new MediaRoute(relay, sessionId, durationMs, null);
        }
        for (MediaRelay relay : plan.mediaRelays) {
            if (relay.mode != RelayMode.TOKEN_INCENTIVIZED || access == null
                    || relay.expiresAtMs <= nowMs || !relay.nodeId.equals(access.relayNodeId)
                    || !sessionId.equals(access.sessionId) || access.expiresAtMs <= nowMs
                    || access.maxDurationMs < durationMs) continue;
            return new MediaRoute(relay, sessionId, durationMs, access.token);
        }
        throw new IOException("No decentralized media relay available");
    }

    private static List<byte[]> copyFrames(List<byte[]> frames) {
        List<byte[]> copy = new ArrayList<>(frames.size());
        for (byte[] frame : frames) copy.add(frame.clone());
        return Collections.unmodifiableList(copy);
    }

    private static <T> CompletableFuture<T> failedGeneric(Throwable error) {
        CompletableFuture<T> future = new CompletableFuture<>();
        future.completeExceptionally(error);
        return future;
    }

    private static <T> CompletableFuture<T> failed(Throwable error) {
        return failedGeneric(error);
    }

    private static Throwable unwrap(Throwable error) {
        return error instanceof CompletionException && error.getCause() != null
                ? unwrap(error.getCause()) : error;
    }

    private static boolean validLocator(String value) {
        if (value == null || value.isEmpty() || value.length() > 128) return false;
        for (char c : value.toCharArray()) {
            if (!(Character.isLetterOrDigit(c) || c == '.' || c == ':' || c == '_'
                    || c == '-')) return false;
        }
        return true;
    }

    private static boolean validEndpoint(String value, String scheme) {
        if (value == null || value.isEmpty() || value.length() > 512
                || !value.startsWith(scheme)) return false;
        for (char c : value.toCharArray()) {
            if (Character.isISOControl(c) || Character.isWhitespace(c) || c == '#') return false;
        }
        return true;
    }

    private static boolean validUuid(String value) {
        return value != null && value.matches(
                "[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}")
                && !value.equals("00000000-0000-0000-0000-000000000000");
    }

    private static boolean validCid(String value) {
        if (value == null || value.length() != 59 || value.charAt(0) != 'b') return false;
        for (int index = 1; index < value.length(); index++) {
            char c = value.charAt(index);
            if (!((c >= 'a' && c <= 'z') || (c >= '2' && c <= '7'))) return false;
        }
        return true;
    }

    private static String cidForCiphertext(byte[] ciphertext) throws IOException {
        if (ciphertext == null || ciphertext.length == 0 || ciphertext.length > MAX_CHUNK_BYTES)
            throw new IOException("Invalid ciphertext chunk");
        byte[] digest;
        try {
            digest = MessageDigest.getInstance("SHA-256").digest(ciphertext);
        } catch (Exception error) {
            throw new IOException("SHA-256 unavailable", error);
        }
        byte[] binary = new byte[36];
        binary[0] = 1;
        binary[1] = 0x55;
        binary[2] = 0x12;
        binary[3] = 0x20;
        System.arraycopy(digest, 0, binary, 4, digest.length);
        return "b" + base32(binary);
    }

    private static String base32(byte[] value) {
        final char[] alphabet = "abcdefghijklmnopqrstuvwxyz234567".toCharArray();
        StringBuilder output = new StringBuilder(58);
        int accumulator = 0;
        int bits = 0;
        for (byte item : value) {
            accumulator = (accumulator << 8) | (item & 0xff);
            bits += 8;
            while (bits >= 5) {
                bits -= 5;
                output.append(alphabet[(accumulator >> bits) & 31]);
                accumulator = bits == 0 ? 0 : accumulator & ((1 << bits) - 1);
            }
        }
        if (bits > 0) output.append(alphabet[(accumulator << (5 - bits)) & 31]);
        return output.toString();
    }
}
