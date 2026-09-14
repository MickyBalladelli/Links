import Foundation

/// Media mode selected by the call service. Live streams use the same
/// encrypted media path as calls, but the native engine may expose a
/// publisher-only or subscriber-only UI for that mode.
public enum IOSCallMode: String, Sendable {
    case voice
    case video
    case liveStream = "live-stream"
}

public enum IOSCallState: String, Sendable {
    case idle
    case preparing
    case joining
    case negotiating
    case connected
    case ended
    case failed
}

public struct IOSCallPlacement: Equatable, Sendable {
    public let roomName: String
    public let region: String
    public let endpoint: URL
    public let accessToken: String
    public let requireSFrame: Bool

    public init(roomName: String, region: String, endpoint: URL,
                accessToken: String, requireSFrame: Bool = true) throws {
        guard Self.validOpaqueRoomName(roomName), Self.validRegion(region),
              Self.validEndpoint(endpoint), !accessToken.isEmpty,
              accessToken.utf8.count <= 4_096, requireSFrame else {
            throw IOSCallError.invalidPlacement
        }
        self.roomName = roomName
        self.region = region
        self.endpoint = endpoint
        self.accessToken = accessToken
        self.requireSFrame = requireSFrame
    }

    private static func validOpaqueRoomName(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        guard (16...128).contains(bytes.count) else { return false }
        return bytes.allSatisfy { byte in
            (byte >= 48 && byte <= 57)
                || (byte >= 65 && byte <= 90)
                || (byte >= 97 && byte <= 122)
                || byte == 45 || byte == 46 || byte == 95
        }
    }

    private static func validRegion(_ value: String) -> Bool {
        let bytes = Array(value.utf8)
        guard !bytes.isEmpty, bytes.count <= 128 else { return false }
        return bytes.allSatisfy { byte in
            (byte >= 48 && byte <= 57)
                || (byte >= 65 && byte <= 90)
                || (byte >= 97 && byte <= 122)
                || byte == 45 || byte == 46 || byte == 95 || byte == 58
        }
    }

    private static func validEndpoint(_ value: URL) -> Bool {
        value.scheme?.lowercased() == "wss"
            && value.host?.isEmpty == false
            && value.user == nil
            && value.password == nil
            && value.query == nil
            && value.fragment == nil
    }
}

public enum IOSCallSignalKind: String, Sendable {
    case offer
    case answer
    case iceCandidate = "ice-candidate"
}

public struct IOSCallSignal: Sendable {
    public let sessionID: String
    public let kind: IOSCallSignalKind
    public let sdp: String
    public let sdpMid: String?
    public let sdpMLineIndex: Int?

    public init(sessionID: String, kind: IOSCallSignalKind, sdp: String,
                sdpMid: String? = nil, sdpMLineIndex: Int? = nil) throws {
        guard IOSClient.isCanonicalUUID(sessionID), !sdp.isEmpty,
              sdp.utf8.count <= 256 * 1024,
              sdpMid.map({ $0.utf8.count <= 256 }) ?? true,
              sdpMLineIndex.map({ $0 >= 0 }) ?? true else {
            throw IOSCallError.invalidSignal
        }
        self.sessionID = sessionID
        self.kind = kind
        self.sdp = sdp
        self.sdpMid = sdpMid
        self.sdpMLineIndex = sdpMLineIndex
    }
}

public struct IOSCallEpochKey: Sendable {
    public let mediaSessionID: String
    public let keyID: UInt64
    public let epoch: UInt64
    public private(set) var key: Data

    public init(mediaSessionID: String, keyID: UInt64, epoch: UInt64,
                key: Data) throws {
        guard IOSClient.isCanonicalUUID(mediaSessionID), key.count == 16,
              key.contains(where: { $0 != 0 }) else {
            throw IOSCallError.invalidKeyMaterial
        }
        self.mediaSessionID = mediaSessionID
        self.keyID = keyID
        self.epoch = epoch
        self.key = key
    }

    public mutating func wipe() {
        key.resetBytes(in: 0..<key.count)
    }
}

/// LiveKit/Mediasoup adapter. It handles provider signaling only. It never
/// receives MLS keys or plaintext media.
public protocol IOSCallSignaling: AnyObject {
    func join(placement: IOSCallPlacement, sessionID: String,
              mediaSessionID: String) async throws
    func send(_ signal: IOSCallSignal) async throws
    @discardableResult
    func subscribe(_ handler: @escaping (IOSCallSignal) -> Void) -> () -> Void
    func leave() async
}

/// The MLS host owns authenticated control messages and durable epoch state.
public protocol IOSCallMLSKeyProvider: AnyObject {
    func createInitialKey(mediaSessionID: String) async throws -> IOSCallEpochKey
    func publishEpochKey(_ key: IOSCallEpochKey) async throws
    @discardableResult
    func subscribe(_ handler: @escaping (IOSCallEpochKey) -> Void) -> () -> Void
}

/// Native WebRTC/LiveKit binding. The concrete implementation attaches
/// platform SFrame transforms; this boundary keeps provider SDK code out of
/// the shared call state machine.
public protocol IOSCallMediaEngine: AnyObject {
    func prepareSFrame(_ key: IOSCallEpochKey) async throws
    func installSFrame(_ key: IOSCallEpochKey) async throws
    func configure(mode: IOSCallMode) async throws
    func createOffer() async throws -> String
    func createAnswer() async throws -> String
    func setRemoteDescription(_ signal: IOSCallSignal) async throws
    func addIceCandidate(_ signal: IOSCallSignal) async throws
    func attachSFrameToReceivers() async throws
    func close()
}

/// Mobile call/live-stream orchestration. The host supplies the native
/// WebRTC engine, while this class enforces MLS-before-SDP and SFrame-before-
/// media ordering on iOS.
public final class IOSCallSession {
    private let mode: IOSCallMode
    private let placement: IOSCallPlacement
    private let signaling: any IOSCallSignaling
    private let mls: any IOSCallMLSKeyProvider
    private let mediaEngine: any IOSCallMediaEngine
    private let sessionID: String
    private let mediaSessionID: String
    private let onState: (IOSCallState) -> Void
    private let onError: (Error) -> Void
    private var unsubscribeSignals: (() -> Void)?
    private var unsubscribeMLS: (() -> Void)?
    private var pendingCandidates: [IOSCallSignal] = []
    private var remoteDescriptionSet = false
    private var joined = false
    private var currentState = IOSCallState.idle

    public init(mode: IOSCallMode, placement: IOSCallPlacement,
                signaling: any IOSCallSignaling, mls: any IOSCallMLSKeyProvider,
                mediaEngine: any IOSCallMediaEngine,
                sessionID: String = UUID().uuidString.lowercased(),
                mediaSessionID: String? = nil,
                onState: @escaping (IOSCallState) -> Void = { _ in },
                onError: @escaping (Error) -> Void = { _ in }) throws {
        guard IOSClient.isCanonicalUUID(sessionID) else {
            throw IOSCallError.invalidSession
        }
        let selectedMediaSessionID = mediaSessionID ?? sessionID
        guard IOSClient.isCanonicalUUID(selectedMediaSessionID) else {
            throw IOSCallError.invalidSession
        }
        self.mode = mode
        self.placement = placement
        self.signaling = signaling
        self.mls = mls
        self.mediaEngine = mediaEngine
        self.sessionID = sessionID
        self.mediaSessionID = selectedMediaSessionID
        self.onState = onState
        self.onError = onError
    }

    public var state: IOSCallState { currentState }
    public var callMode: IOSCallMode { mode }
    public var callSessionID: String { sessionID }
    public var callMediaSessionID: String { mediaSessionID }

    public func start() async throws {
        guard currentState == .idle else { throw IOSCallError.alreadyStarted }
        setState(.preparing)
        do {
            var initial = try await mls.createInitialKey(mediaSessionID: mediaSessionID)
            defer { initial.wipe() }
            try Self.validateKey(initial, expected: mediaSessionID)
            try await mediaEngine.prepareSFrame(initial)
            try await publishEpochKey(initial)
            try await mediaEngine.configure(mode: mode)
            subscribeToControl()
            setState(.joining)
            joined = true
            try await signaling.join(placement: placement, sessionID: sessionID,
                                     mediaSessionID: mediaSessionID)
            setState(.negotiating)
            let offer = try await mediaEngine.createOffer()
            let signal = try IOSCallSignal(sessionID: sessionID, kind: .offer, sdp: offer)
            try await signaling.send(signal)
        } catch {
            await fail(error)
            throw error
        }
    }

    public func publishSFrameEpochKey(_ key: IOSCallEpochKey) async throws {
        guard currentState != .ended, currentState != .failed else {
            throw IOSCallError.closed
        }
        try Self.validateKey(key, expected: mediaSessionID)
        var working = key
        defer { working.wipe() }
        try await mediaEngine.installSFrame(working)
        try await publishEpochKey(working)
    }

    public func installSFrameEpochKey(_ key: IOSCallEpochKey) async throws {
        guard currentState != .ended, currentState != .failed else {
            throw IOSCallError.closed
        }
        try Self.validateKey(key, expected: mediaSessionID)
        var working = key
        defer { working.wipe() }
        try await mediaEngine.installSFrame(working)
    }

    public func handleSignal(_ signal: IOSCallSignal) async throws {
        guard joined else { throw IOSCallError.notJoined }
        guard signal.sessionID == sessionID else { throw IOSCallError.invalidSignal }
        switch signal.kind {
        case .iceCandidate:
            if !remoteDescriptionSet {
                pendingCandidates.append(signal)
            } else {
                try await mediaEngine.addIceCandidate(signal)
            }
        case .offer:
            try await mediaEngine.setRemoteDescription(signal)
            remoteDescriptionSet = true
            try await mediaEngine.attachSFrameToReceivers()
            try await flushCandidates()
            let answer = try await mediaEngine.createAnswer()
            let response = try IOSCallSignal(sessionID: sessionID, kind: .answer, sdp: answer)
            try await signaling.send(response)
        case .answer:
            try await mediaEngine.setRemoteDescription(signal)
            remoteDescriptionSet = true
            try await mediaEngine.attachSFrameToReceivers()
            try await flushCandidates()
        }
    }

    public func stop() async {
        guard currentState != .ended else { return }
        joined = false
        unsubscribeSignals?()
        unsubscribeSignals = nil
        unsubscribeMLS?()
        unsubscribeMLS = nil
        await signaling.leave()
        mediaEngine.close()
        pendingCandidates.removeAll()
        setState(.ended)
    }

    private func publishEpochKey(_ key: IOSCallEpochKey) async throws {
        var outbound = key
        defer { outbound.wipe() }
        try await mls.publishEpochKey(outbound)
    }

    private func subscribeToControl() {
        unsubscribeSignals = signaling.subscribe { [weak self] signal in
            guard let self else { return }
            Task {
                do {
                    try await self.handleSignal(signal)
                } catch {
                    await self.fail(error)
                }
            }
        }
        unsubscribeMLS = mls.subscribe { [weak self] key in
            guard let self else { return }
            Task {
                do {
                    try await self.installSFrameEpochKey(key)
                } catch {
                    await self.fail(error)
                }
            }
        }
    }

    private func flushCandidates() async throws {
        let candidates = pendingCandidates
        pendingCandidates.removeAll()
        for candidate in candidates {
            try await mediaEngine.addIceCandidate(candidate)
        }
    }

    private func fail(_ error: Error) async {
        guard currentState != .ended, currentState != .failed else { return }
        joined = false
        unsubscribeSignals?()
        unsubscribeSignals = nil
        unsubscribeMLS?()
        unsubscribeMLS = nil
        pendingCandidates.removeAll()
        setState(.failed)
        await signaling.leave()
        mediaEngine.close()
        onError(error)
    }

    private func setState(_ state: IOSCallState) {
        currentState = state
        onState(state)
    }

    private static func validateKey(_ key: IOSCallEpochKey, expected: String) throws {
        guard key.mediaSessionID == expected, key.key.count == 16,
              key.key.contains(where: { $0 != 0 }) else {
            throw IOSCallError.invalidKeyMaterial
        }
    }
}

public enum IOSCallError: Error {
    case invalidPlacement
    case invalidSession
    case invalidSignal
    case invalidKeyMaterial
    case alreadyStarted
    case notJoined
    case closed
}
