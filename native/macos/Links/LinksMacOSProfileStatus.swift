import Foundation
import LinksKeyStore

/// Machine-readable, profile-local readiness output for launchers and smoke
/// runners. It contains only lifecycle state and process metadata.
@MainActor
final class LinksMacOSProfileStatus {
    enum State: String, Codable {
        case launching
        case identityRequired = "identity-required"
        case authenticationRequired = "authentication-required"
        case notConfigured = "not-configured"
        case offline
        case connecting
        case ready
        case reconnecting
        case retryingOutbox = "retrying-outbox"
        case staleCursor = "stale-cursor"
        case authenticationExpired = "authentication-expired"
        case sendFailed = "send-failed"
        case dependencyOutage = "dependency-outage"
        case stopped
        case failed
    }

    struct Snapshot: Codable {
        let schema = "links-macos-profile-status-v1"
        let profile: String
        let state: State
        let authenticated: Bool
        let connected: Bool
        let pid: Int32
        let updatedAt: Date

        enum CodingKeys: String, CodingKey {
            case schema
            case profile
            case state
            case authenticated
            case connected
            case pid
            case updatedAt = "updated_at"
        }
    }

    let url: URL
    private let profile: String
    private let fileManager: FileManager
    private let encoder: JSONEncoder

    init(root: MacOSProfileRoot, fileManager: FileManager = .default) throws {
        self.profile = root.profile.name
        self.fileManager = fileManager
        self.url = root.url.appendingPathComponent("status.json", isDirectory: false)
        self.encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601

        try fileManager.createDirectory(
            at: root.url,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: NSNumber(value: 0o700)])
        if fileManager.fileExists(atPath: url.path) {
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)], ofItemAtPath: url.path)
        }
    }

    func write(_ state: State, authenticated: Bool, connected: Bool) {
        let snapshot = Snapshot(
            profile: profile,
            state: state,
            authenticated: authenticated,
            connected: connected,
            pid: ProcessInfo.processInfo.processIdentifier,
            updatedAt: Date())
        guard let data = try? encoder.encode(snapshot) else { return }
        do {
            try data.write(to: url, options: Data.WritingOptions.atomic)
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)], ofItemAtPath: url.path)
        } catch {
            // Readiness output is auxiliary. A failed status write must not
            // stop the encrypted client or alter its durable state.
        }
    }
}
