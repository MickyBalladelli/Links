package ai.links.app;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.UUID;
import java.util.concurrent.CompletableFuture;
import java.util.function.Consumer;

/** Voice, video, and live-stream media modes supported by the Android host. */
public final class AndroidCallSession {
    public enum Mode { VOICE, VIDEO, LIVE_STREAM }

    public enum State {
        IDLE, PREPARING, JOINING, NEGOTIATING, CONNECTED, ENDED, FAILED
    }

    public enum SignalKind { OFFER, ANSWER, ICE_CANDIDATE }

    public static final class Placement {
        public final String roomName;
        public final String region;
        public final String endpoint;
        public final String accessToken;
        public final boolean requireSFrame;

        public Placement(String roomName, String region, String endpoint,
                String accessToken, boolean requireSFrame) throws IOException {
            if (!validOpaqueRoomName(roomName) || !validRegion(region)
                    || !validEndpoint(endpoint) || accessToken == null
                    || accessToken.isEmpty()
                    || accessToken.getBytes(StandardCharsets.UTF_8).length > 4_096
                    || !requireSFrame)
                throw new IOException("Invalid call placement");
            this.roomName = roomName;
            this.region = region;
            this.endpoint = endpoint;
            this.accessToken = accessToken;
            this.requireSFrame = requireSFrame;
        }
    }

    public static final class Signal {
        public final String sessionId;
        public final SignalKind kind;
        public final String sdp;
        public final String sdpMid;
        public final Integer sdpMLineIndex;

        public Signal(String sessionId, SignalKind kind, String sdp,
                String sdpMid, Integer sdpMLineIndex) throws IOException {
            requireUuid(sessionId, "call session ID");
            if (kind == null || sdp == null || sdp.isEmpty()
                    || sdp.getBytes(StandardCharsets.UTF_8).length > 256 * 1024
                    || (sdpMid != null && sdpMid.getBytes(StandardCharsets.UTF_8).length > 256)
                    || (sdpMLineIndex != null && sdpMLineIndex < 0))
                throw new IOException("Invalid call signal");
            this.sessionId = sessionId;
            this.kind = kind;
            this.sdp = sdp;
            this.sdpMid = sdpMid;
            this.sdpMLineIndex = sdpMLineIndex;
        }
    }

    public static final class EpochKey {
        public final String mediaSessionId;
        public final long keyId;
        public final long epoch;
        private final byte[] key;

        public EpochKey(String mediaSessionId, long keyId, long epoch, byte[] key)
                throws IOException {
            requireUuid(mediaSessionId, "media session ID");
            if (key == null || key.length != 16 || allZero(key))
                throw new IOException("Invalid SFrame key material");
            this.mediaSessionId = mediaSessionId;
            this.keyId = keyId;
            this.epoch = epoch;
            this.key = key.clone();
        }

        public byte[] copyKey() {
            return key.clone();
        }

        private EpochKey copy() throws IOException {
            return new EpochKey(mediaSessionId, keyId, epoch, key);
        }

        private void wipe() {
            Arrays.fill(key, (byte) 0);
        }
    }

    /** Provider SDK boundary. It handles LiveKit/Mediasoup signaling only. */
    public interface Signaling {
        CompletableFuture<Void> join(Placement placement, String sessionId,
                String mediaSessionId);
        CompletableFuture<Void> send(Signal signal);
        Runnable subscribe(Consumer<Signal> handler);
        CompletableFuture<Void> leave();
    }

    /** MLS application-control boundary. Raw keys never go to Signaling. */
    public interface MlsKeyProvider {
        CompletableFuture<EpochKey> createInitialKey(String mediaSessionId);
        CompletableFuture<Void> publishEpochKey(EpochKey key);
        Runnable subscribe(Consumer<EpochKey> handler);
    }

    /** Native WebRTC binding, including Android SFrame Encoded Transform hooks. */
    public interface MediaEngine {
        CompletableFuture<Void> prepareSFrame(EpochKey key);
        CompletableFuture<Void> installSFrame(EpochKey key);
        CompletableFuture<Void> configure(Mode mode);
        CompletableFuture<String> createOffer();
        CompletableFuture<String> createAnswer();
        CompletableFuture<Void> setRemoteDescription(Signal signal);
        CompletableFuture<Void> addIceCandidate(Signal signal);
        CompletableFuture<Void> attachSFrameToReceivers();
        void close();
    }

    private final Mode mode;
    private final Placement placement;
    private final Signaling signaling;
    private final MlsKeyProvider mls;
    private final MediaEngine mediaEngine;
    private final String sessionId;
    private final String mediaSessionId;
    private final Consumer<State> stateListener;
    private final Consumer<Throwable> errorListener;
    private final Object lock = new Object();
    private final List<Signal> pendingCandidates = new ArrayList<>();
    private Runnable unsubscribeSignals;
    private Runnable unsubscribeMls;
    private State state = State.IDLE;
    private boolean joined;
    private boolean remoteDescriptionSet;

    public AndroidCallSession(Mode mode, Placement placement, Signaling signaling,
            MlsKeyProvider mls, MediaEngine mediaEngine, String sessionId,
            String mediaSessionId, Consumer<State> stateListener,
            Consumer<Throwable> errorListener) throws IOException {
        if (mode == null || placement == null || signaling == null || mls == null
                || mediaEngine == null || stateListener == null || errorListener == null)
            throw new IOException("Invalid call session");
        requireUuid(sessionId, "call session ID");
        String selectedMediaSessionId = mediaSessionId == null ? sessionId : mediaSessionId;
        requireUuid(selectedMediaSessionId, "media session ID");
        this.mode = mode;
        this.placement = placement;
        this.signaling = signaling;
        this.mls = mls;
        this.mediaEngine = mediaEngine;
        this.sessionId = sessionId;
        this.mediaSessionId = selectedMediaSessionId;
        this.stateListener = stateListener;
        this.errorListener = errorListener;
    }

    public State state() {
        synchronized (lock) { return state; }
    }

    public Mode mode() {
        return mode;
    }

    public String sessionId() {
        return sessionId;
    }

    public String mediaSessionId() {
        return mediaSessionId;
    }

    /** Enforce MLS key setup and SFrame attachment before SDP is sent. */
    public CompletableFuture<Void> start() {
        synchronized (lock) {
            if (state != State.IDLE) return failed(new IOException("Call already started"));
            setStateLocked(State.PREPARING);
        }
        CompletableFuture<Void> started;
        try {
            started = mls.createInitialKey(mediaSessionId).thenCompose(initial -> {
                try {
                    validateKey(initial);
                } catch (Throwable error) {
                    if (initial != null) initial.wipe();
                    return failed(error);
                }
                return mediaEngine.prepareSFrame(initial)
                        .thenCompose(ignored -> publishEpochKey(initial))
                        .thenCompose(ignored -> mediaEngine.configure(mode))
                        .whenComplete((ignored, error) -> initial.wipe());
            }).thenCompose(ignored -> {
                subscribeToControl();
                synchronized (lock) {
                    joined = true;
                    setStateLocked(State.JOINING);
                }
                return signaling.join(placement, sessionId, mediaSessionId);
            }).thenCompose(ignored -> {
                setState(State.NEGOTIATING);
                return mediaEngine.createOffer();
            }).thenCompose(offer -> {
                try {
                    return signaling.send(new Signal(sessionId, SignalKind.OFFER, offer,
                            null, null));
                } catch (Throwable error) {
                    return failed(error);
                }
            });
        } catch (Throwable error) {
            fail(error);
            return failed(error);
        }
        return started.whenComplete((ignored, error) -> {
            if (error != null) fail(error);
        });
    }

    public CompletableFuture<Void> publishSFrameEpochKey(EpochKey key) {
        try {
            validateKey(key);
            EpochKey working = key.copy();
            return mediaEngine.installSFrame(working)
                    .thenCompose(ignored -> publishEpochKey(working))
                    .whenComplete((ignored, error) -> working.wipe());
        } catch (Throwable error) {
            return failed(error);
        }
    }

    public CompletableFuture<Void> installSFrameEpochKey(EpochKey key) {
        try {
            validateKey(key);
            EpochKey working = key.copy();
            return mediaEngine.installSFrame(working)
                    .whenComplete((ignored, error) -> working.wipe());
        } catch (Throwable error) {
            return failed(error);
        }
    }

    /** Provider callbacks feed SDP and ICE into this method. */
    public CompletableFuture<Void> handleSignal(Signal signal) {
        try {
            if (!joined || signal == null || !sessionId.equals(signal.sessionId))
                throw new IOException("Invalid call signal state");
            if (signal.kind == SignalKind.ICE_CANDIDATE) {
                synchronized (lock) {
                    if (!remoteDescriptionSet) {
                        pendingCandidates.add(signal);
                        return CompletableFuture.completedFuture(null);
                    }
                }
                return mediaEngine.addIceCandidate(signal);
            }
            return mediaEngine.setRemoteDescription(signal)
                    .thenCompose(ignored -> {
                        synchronized (lock) { remoteDescriptionSet = true; }
                        return mediaEngine.attachSFrameToReceivers();
                    })
                    .thenCompose(ignored -> flushCandidates())
                    .thenCompose(ignored -> {
                        if (signal.kind != SignalKind.OFFER)
                            return CompletableFuture.completedFuture(null);
                        return mediaEngine.createAnswer().thenCompose(answer -> {
                            try {
                                return signaling.send(new Signal(sessionId, SignalKind.ANSWER,
                                        answer, null, null));
                            } catch (Throwable error) {
                                return failed(error);
                            }
                        });
                    })
                    .whenComplete((ignored, error) -> {
                        if (error != null) fail(error);
                    });
        } catch (Throwable error) {
            fail(error);
            return failed(error);
        }
    }

    public CompletableFuture<Void> stop() {
        synchronized (lock) {
            if (state == State.ENDED) return CompletableFuture.completedFuture(null);
            joined = false;
            if (unsubscribeSignals != null) unsubscribeSignals.run();
            if (unsubscribeMls != null) unsubscribeMls.run();
            unsubscribeSignals = null;
            unsubscribeMls = null;
            pendingCandidates.clear();
        }
        CompletableFuture<Void> leave;
        try {
            leave = signaling.leave();
        } catch (Throwable error) {
            leave = CompletableFuture.completedFuture(null);
        }
        return leave.handle((ignored, error) -> {
            mediaEngine.close();
            setState(State.ENDED);
            return null;
        });
    }

    private CompletableFuture<Void> publishEpochKey(EpochKey key) {
        try {
            EpochKey outbound = key.copy();
            return mls.publishEpochKey(outbound)
                    .whenComplete((ignored, error) -> outbound.wipe());
        } catch (Throwable error) {
            return failed(error);
        }
    }

    private CompletableFuture<Void> flushCandidates() {
        List<Signal> candidates;
        synchronized (lock) {
            candidates = new ArrayList<>(pendingCandidates);
            pendingCandidates.clear();
        }
        CompletableFuture<Void> result = CompletableFuture.completedFuture(null);
        for (Signal candidate : candidates)
            result = result.thenCompose(ignored -> mediaEngine.addIceCandidate(candidate));
        return result;
    }

    private void subscribeToControl() {
        unsubscribeSignals = signaling.subscribe(signal -> handleSignal(signal));
        unsubscribeMls = mls.subscribe(key -> installSFrameEpochKey(key));
    }

    private void fail(Throwable error) {
        synchronized (lock) {
            if (state == State.ENDED || state == State.FAILED) return;
            joined = false;
            if (unsubscribeSignals != null) unsubscribeSignals.run();
            if (unsubscribeMls != null) unsubscribeMls.run();
            unsubscribeSignals = null;
            unsubscribeMls = null;
            pendingCandidates.clear();
            setStateLocked(State.FAILED);
        }
        mediaEngine.close();
        try { signaling.leave(); } catch (Throwable ignored) { }
        errorListener.accept(error);
    }

    private void setState(State next) {
        synchronized (lock) { setStateLocked(next); }
    }

    private void setStateLocked(State next) {
        state = next;
        stateListener.accept(next);
    }

    private void validateKey(EpochKey key) throws IOException {
        if (key == null || !mediaSessionId.equals(key.mediaSessionId)
                || key.keyId < 0 || key.epoch < 0) throw new IOException("Invalid SFrame key material");
        byte[] copy = key.copyKey();
        try {
            if (copy.length != 16 || allZero(copy)) throw new IOException("Invalid SFrame key material");
        } finally {
            Arrays.fill(copy, (byte) 0);
        }
    }

    private static CompletableFuture<Void> failed(Throwable error) {
        CompletableFuture<Void> result = new CompletableFuture<>();
        result.completeExceptionally(error);
        return result;
    }

    private static boolean allZero(byte[] value) {
        for (byte item : value) if (item != 0) return false;
        return true;
    }

    private static boolean validOpaqueRoomName(String value) {
        if (value == null) return false;
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        if (bytes.length < 16 || bytes.length > 128) return false;
        for (byte item : bytes) {
            int current = item & 0xff;
            if (!((current >= '0' && current <= '9')
                    || (current >= 'A' && current <= 'Z')
                    || (current >= 'a' && current <= 'z')
                    || current == '-' || current == '.' || current == '_'))
                return false;
        }
        return true;
    }

    private static boolean validRegion(String value) {
        if (value == null) return false;
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        if (bytes.length == 0 || bytes.length > 128) return false;
        for (byte item : bytes) {
            int current = item & 0xff;
            if (!((current >= '0' && current <= '9')
                    || (current >= 'A' && current <= 'Z')
                    || (current >= 'a' && current <= 'z')
                    || current == '-' || current == '.' || current == '_'
                    || current == ':'))
                return false;
        }
        return true;
    }

    private static boolean validEndpoint(String value) {
        if (value == null || !value.startsWith("wss://")) return false;
        if (value.indexOf('?') >= 0 || value.indexOf('#') >= 0) return false;
        String remainder = value.substring("wss://".length());
        String authority = remainder.split("/", 2)[0];
        return !authority.isEmpty() && authority.indexOf('@') < 0
                && authority.indexOf('?') < 0 && authority.indexOf('#') < 0
                && authority.indexOf(' ') < 0 && authority.indexOf('\t') < 0
                && authority.indexOf('\n') < 0 && authority.indexOf('\r') < 0;
    }

    private static void requireUuid(String value, String field) throws IOException {
        try {
            UUID parsed = UUID.fromString(value);
            if (parsed.getMostSignificantBits() == 0 && parsed.getLeastSignificantBits() == 0
                    || !parsed.toString().equals(value)) throw new IllegalArgumentException();
        } catch (RuntimeException error) {
            throw new IOException("Invalid " + field, error);
        }
    }
}
