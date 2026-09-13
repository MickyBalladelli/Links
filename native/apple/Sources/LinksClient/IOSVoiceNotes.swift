import AVFoundation
import CryptoKit
import Foundation

public enum IOSVoiceNoteContainer: Int, Sendable {
    case ogg = 1
    case opus = 2

    public var mimeType: String {
        switch self {
        case .ogg: return "audio/ogg"
        case .opus: return "audio/ogg; codecs=opus"
        }
    }
}

public struct IOSVoiceNoteProfile: Equatable, Sendable {
    public let container: IOSVoiceNoteContainer
    public let bitrateKbps: UInt32
    public let sampleRateHz: UInt32
    public let channels: UInt32
    public let frameDurationMs: UInt32

    public init(container: IOSVoiceNoteContainer = .ogg, bitrateKbps: UInt32 = 24,
                sampleRateHz: UInt32 = 48_000, channels: UInt32 = 1,
                frameDurationMs: UInt32 = 20) throws {
        guard (16...24).contains(bitrateKbps),
              [8_000, 12_000, 16_000, 24_000, 48_000].contains(sampleRateHz),
              (1...2).contains(channels), frameDurationMs == 20 else {
            throw IOSVoiceNoteError.invalidProfile
        }
        self.container = container
        self.bitrateKbps = bitrateKbps
        self.sampleRateHz = sampleRateHz
        self.channels = channels
        self.frameDurationMs = frameDurationMs
    }
}

public struct IOSVoiceNoteMetadata: Equatable, Sendable {
    public let attachmentID: String
    public let mimeType: String
    public let ciphertextSizeBytes: UInt64
    public let contentKey: Data
    public let nonce: Data
    public let ciphertextSHA256: Data
    public let durationMs: UInt64
    public let profile: IOSVoiceNoteProfile

    public init(attachmentID: String, mimeType: String, ciphertextSizeBytes: UInt64,
                contentKey: Data, nonce: Data, ciphertextSHA256: Data,
                durationMs: UInt64, profile: IOSVoiceNoteProfile) throws {
        guard IOSClient.isCanonicalUUID(attachmentID), mimeType == profile.container.mimeType,
              (16...16 * 1024 * 1024 + 16).contains(ciphertextSizeBytes),
              contentKey.count == 32, nonce.count == 12, ciphertextSHA256.count == 32,
              durationMs > 0 else {
            throw IOSVoiceNoteError.invalidMetadata
        }
        self.attachmentID = attachmentID
        self.mimeType = mimeType
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.contentKey = contentKey
        self.nonce = nonce
        self.ciphertextSHA256 = ciphertextSHA256
        self.durationMs = durationMs
        self.profile = profile
    }
}

public struct IOSEncryptedVoiceNote: Sendable {
    public let metadata: IOSVoiceNoteMetadata
    public let ciphertext: Data

    public init(metadata: IOSVoiceNoteMetadata, ciphertext: Data) throws {
        guard ciphertext.count == Int(metadata.ciphertextSizeBytes),
              Data(SHA256.hash(data: ciphertext)) == metadata.ciphertextSHA256 else {
            throw IOSVoiceNoteError.integrityFailure
        }
        self.metadata = metadata
        self.ciphertext = ciphertext
    }
}

public struct IOSVoiceNoteUploadReceipt: Equatable, Sendable {
    public let attachmentID: String
    public let ciphertextSizeBytes: UInt64
    public let ciphertextSHA256: Data

    public init(attachmentID: String, ciphertextSizeBytes: UInt64,
                ciphertextSHA256: Data) throws {
        guard IOSClient.isCanonicalUUID(attachmentID),
              (16...16 * 1024 * 1024 + 16).contains(ciphertextSizeBytes),
              ciphertextSHA256.count == 32 else {
            throw IOSVoiceNoteError.invalidUploadReceipt
        }
        self.attachmentID = attachmentID
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.ciphertextSHA256 = ciphertextSHA256
    }

    public func matches(_ metadata: IOSVoiceNoteMetadata) -> Bool {
        attachmentID == metadata.attachmentID
            && ciphertextSizeBytes == metadata.ciphertextSizeBytes
            && ciphertextSHA256 == metadata.ciphertextSHA256
    }
}

public protocol IOSVoiceNoteUploader: AnyObject {
    /// Implement with authenticated TLS. The body contains ciphertext only.
    func upload(_ note: IOSEncryptedVoiceNote, accessToken: String)
        throws -> IOSVoiceNoteUploadReceipt
    /// Return the exact opaque ciphertext identified by the private metadata.
    func download(_ metadata: IOSVoiceNoteMetadata, accessToken: String) throws -> Data
}

public enum IOSVoiceNoteError: Error {
    case invalidProfile
    case invalidMetadata
    case invalidUploadReceipt
    case integrityFailure
    case permissionRequired
    case recordingUnavailable
    case recordingNotActive
    case coreUnavailable
}

#if os(iOS)
/// AVAudioRecorder captures bounded temporary PCM. The shared Rust core turns
/// those frames into the canonical Ogg Opus stream before encryption.
public final class IOSVoiceNoteRecorder: NSObject {
    public let profile: IOSVoiceNoteProfile
    private let outputURL: URL
    private var recorder: AVAudioRecorder?
    private var startedAt: Date?

    public static func requestRecordPermission(_ completion: @escaping (Bool) -> Void) {
        AVAudioSession.sharedInstance().requestRecordPermission(completion)
    }

    public init(profile: IOSVoiceNoteProfile,
                directory: URL = FileManager.default.temporaryDirectory) throws {
        self.profile = profile
        self.outputURL = directory.appendingPathComponent(
            "links-voice-\(UUID().uuidString).caf")
        super.init()
    }

    public var isRecording: Bool { recorder?.isRecording == true }

    public func start() throws {
        guard recorder == nil else { throw IOSVoiceNoteError.recordingUnavailable }
        let audioSession = AVAudioSession.sharedInstance()
        guard audioSession.recordPermission == .granted else {
            throw IOSVoiceNoteError.permissionRequired
        }
        try audioSession.setCategory(.record, mode: .voiceChat, options: [.allowBluetooth])
        try audioSession.setActive(true)
        let settings: [String: Any] = [
            AVFormatIDKey: kAudioFormatLinearPCM,
            AVSampleRateKey: profile.sampleRateHz,
            AVNumberOfChannelsKey: profile.channels,
            AVLinearPCMBitDepthKey: 16,
            AVLinearPCMIsFloatKey: false,
            AVLinearPCMIsBigEndianKey: false,
            AVLinearPCMIsNonInterleaved: false
        ]
        let created = try AVAudioRecorder(url: outputURL, settings: settings)
        guard created.prepareToRecord(), created.record() else {
            try? audioSession.setActive(false, options: .notifyOthersOnDeactivation)
            throw IOSVoiceNoteError.recordingUnavailable
        }
        recorder = created
        startedAt = Date()
    }

    public func stop() throws -> (url: URL, durationMs: UInt64) {
        guard let active = recorder, let startedAt else {
            throw IOSVoiceNoteError.recordingNotActive
        }
        active.stop()
        recorder = nil
        self.startedAt = nil
        try? AVAudioSession.sharedInstance().setActive(
            false, options: .notifyOthersOnDeactivation)
        guard FileManager.default.fileExists(atPath: outputURL.path) else {
            throw IOSVoiceNoteError.recordingUnavailable
        }
        let durationMs = max(1, UInt64(Date().timeIntervalSince(startedAt) * 1_000))
        return (outputURL, durationMs)
    }

    public func cancel() {
        recorder?.stop()
        recorder = nil
        startedAt = nil
        try? AVAudioSession.sharedInstance().setActive(
            false, options: .notifyOthersOnDeactivation)
        try? FileManager.default.removeItem(at: outputURL)
    }

    deinit { cancel() }
}

/// Playback consumes PCM returned by the shared core. This avoids depending
/// on whether a particular iOS release exposes an Ogg Opus AVAudioPlayer path.
public final class IOSVoiceNotePlayback: @unchecked Sendable {
    private let engine = AVAudioEngine()
    private let player = AVAudioPlayerNode()
    private let lock = NSLock()
    private var stopped = false

    public init(pcm: [Int16], profile: IOSVoiceNoteProfile) throws {
        let channelCount = AVAudioChannelCount(profile.channels)
        guard !pcm.isEmpty, pcm.count % Int(profile.channels) == 0,
              let format = AVAudioFormat(commonFormat: .pcmFormatFloat32,
                                         sampleRate: Double(profile.sampleRateHz),
                                         channels: channelCount, interleaved: false),
              let buffer = AVAudioPCMBuffer(
                pcmFormat: format,
                frameCapacity: AVAudioFrameCount(pcm.count / Int(profile.channels))) else {
            throw IOSVoiceNoteError.recordingUnavailable
        }
        buffer.frameLength = buffer.frameCapacity
        guard let floatChannels = buffer.floatChannelData else {
            throw IOSVoiceNoteError.recordingUnavailable
        }
        let frames = Int(buffer.frameLength)
        for channel in 0..<Int(profile.channels) {
            for frame in 0..<frames {
                let sample = pcm[frame * Int(profile.channels) + channel]
                floatChannels[channel][frame] = Float(sample) / Float(Int16.max)
            }
        }

        let audioSession = AVAudioSession.sharedInstance()
        try audioSession.setCategory(.playback, mode: .voiceChat)
        try audioSession.setActive(true)
        engine.attach(player)
        engine.connect(player, to: engine.mainMixerNode, format: format)
        player.scheduleBuffer(buffer) { [weak self] in self?.stop() }
        engine.prepare()
        do {
            try engine.start()
            player.play()
        } catch {
            stop()
            throw error
        }
    }

    public func stop() {
        lock.lock()
        guard !stopped else {
            lock.unlock()
            return
        }
        stopped = true
        lock.unlock()
        player.stop()
        engine.stop()
        engine.reset()
        try? AVAudioSession.sharedInstance().setActive(
            false, options: .notifyOthersOnDeactivation)
    }

    deinit { stop() }
}

/// End-to-end iOS voice-note orchestration around the already-connected
/// IOSDirectMessaging shared core.
public final class IOSVoiceNoteSession {
    private let client: IOSClient
    private let messaging: IOSDirectMessaging
    private let uploader: any IOSVoiceNoteUploader
    private let profile: IOSVoiceNoteProfile
    private var recorder: IOSVoiceNoteRecorder?

    public init(client: IOSClient, messaging: IOSDirectMessaging,
                uploader: any IOSVoiceNoteUploader,
                profile: IOSVoiceNoteProfile = try! IOSVoiceNoteProfile()) {
        self.client = client
        self.messaging = messaging
        self.uploader = uploader
        self.profile = profile
    }

    public func startRecording() throws {
        guard recorder == nil else { throw IOSVoiceNoteError.recordingUnavailable }
        let created = try IOSVoiceNoteRecorder(profile: profile)
        try created.start()
        recorder = created
    }

    public func stopRecordingAndEncrypt() throws -> IOSEncryptedVoiceNote {
        guard let active = recorder else { throw IOSVoiceNoteError.recordingNotActive }
        recorder = nil
        let finished = try active.stop()
        defer { try? FileManager.default.removeItem(at: finished.url) }
        let pcm = try Self.readPCM(url: finished.url, profile: profile)
        let container = try messaging.encodeVoiceNote(pcmFrames: pcm, profile: profile)
        return try messaging.encryptVoiceNote(
            container, attachmentID: UUID().uuidString.lowercased(),
            durationMs: finished.durationMs, profile: profile)
    }

    public func cancelRecording() {
        recorder?.cancel()
        recorder = nil
    }

    public func upload(_ note: IOSEncryptedVoiceNote) throws -> IOSVoiceNoteUploadReceipt {
        let receipt = try uploader.upload(note, accessToken: client.accessToken())
        guard receipt.matches(note.metadata) else {
            throw IOSVoiceNoteError.invalidUploadReceipt
        }
        return receipt
    }

    public func send(_ note: IOSEncryptedVoiceNote, receipt: IOSVoiceNoteUploadReceipt,
                     conversationID: String, recipientUserID: String) throws {
        guard receipt.matches(note.metadata) else {
            throw IOSVoiceNoteError.invalidUploadReceipt
        }
        try messaging.sendVoiceNote(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: note.metadata, receipt: receipt)
    }

    public func downloadAndPlay(_ metadata: IOSVoiceNoteMetadata) throws -> IOSVoiceNotePlayback {
        let ciphertext = try uploader.download(metadata, accessToken: client.accessToken())
        let note = try IOSEncryptedVoiceNote(metadata: metadata, ciphertext: ciphertext)
        var plaintext = try messaging.decryptVoiceNote(
            metadata, ciphertext: note.ciphertext)
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        let pcm = try messaging.decodeVoiceNote(plaintext, profile: metadata.profile)
        return try IOSVoiceNotePlayback(pcm: pcm, profile: metadata.profile)
    }

    private static func readPCM(url: URL, profile: IOSVoiceNoteProfile) throws -> [Int16] {
        let file = try AVAudioFile(forReading: url)
        let format = file.processingFormat
        guard format.commonFormat == .pcmFormatInt16,
              Int(format.sampleRate) == Int(profile.sampleRateHz),
              format.channelCount == AVAudioChannelCount(profile.channels) else {
            throw IOSVoiceNoteError.recordingUnavailable
        }
        let chunkFrames: AVAudioFrameCount = 4_096
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format,
                                            frameCapacity: chunkFrames) else {
            throw IOSVoiceNoteError.recordingUnavailable
        }
        var pcm = [Int16]()
        pcm.reserveCapacity(Int(file.length) * Int(format.channelCount))
        while file.framePosition < file.length {
            try file.read(into: buffer, frameCount: chunkFrames)
            guard buffer.frameLength > 0, let channels = buffer.int16ChannelData else {
                throw IOSVoiceNoteError.recordingUnavailable
            }
            let frames = Int(buffer.frameLength)
            let channelCount = Int(format.channelCount)
            for frame in 0..<frames {
                for channel in 0..<channelCount {
                    pcm.append(channels[channel][frame])
                }
            }
        }
        guard !pcm.isEmpty else { throw IOSVoiceNoteError.recordingUnavailable }
        return pcm
    }
}
#endif

public extension SharedClientCore {
    func encodeVoiceNote(pcmFrames: [Int16], profile: IOSVoiceNoteProfile) throws -> Data {
        throw IOSVoiceNoteError.coreUnavailable
    }

    func encryptVoiceNote(_ container: Data, attachmentID: String,
                          durationMs: UInt64, profile: IOSVoiceNoteProfile)
        throws -> IOSEncryptedVoiceNote {
        throw IOSVoiceNoteError.coreUnavailable
    }

    func decryptVoiceNote(_ metadata: IOSVoiceNoteMetadata, ciphertext: Data) throws -> Data {
        throw IOSVoiceNoteError.coreUnavailable
    }

    func decodeVoiceNote(_ container: Data, profile: IOSVoiceNoteProfile) throws -> [Int16] {
        throw IOSVoiceNoteError.coreUnavailable
    }

    func sendVoiceNote(conversationID: String, recipientUserID: String,
                       metadata: IOSVoiceNoteMetadata,
                       receipt: IOSVoiceNoteUploadReceipt,
                       transport: any IOSCoreTransport) throws {
        throw IOSVoiceNoteError.coreUnavailable
    }
}
