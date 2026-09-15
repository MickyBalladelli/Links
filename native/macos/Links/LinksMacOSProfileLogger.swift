import Foundation
import LinksKeyStore

/// Small profile-local status log. Only fixed event names are accepted, so
/// message text, IDs, tokens, and encrypted payloads cannot enter the file.
@MainActor
final class LinksMacOSProfileLogger {
    enum Event: String {
        case launched = "launch"
        case active = "scene-active"
        case inactive = "scene-inactive"
        case background = "scene-background"
        case stopping = "stopping"
        case identityReady = "identity-ready"
        case connectionStarted = "connection-started"
        case connectionStopped = "connection-stopped"
        case connectionFailed = "connection-failed"
    }

    let url: URL
    private let lock = NSLock()
    private let formatter = ISO8601DateFormatter()

    init(root: MacOSProfileRoot, fileManager: FileManager = .default) throws {
        url = root.logsURL.appendingPathComponent("client.log", isDirectory: false)
        try fileManager.createDirectory(
            at: root.logsURL,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: NSNumber(value: 0o700)])
        if !fileManager.fileExists(atPath: url.path) {
            guard fileManager.createFile(
                atPath: url.path,
                contents: Data(),
                attributes: [.posixPermissions: NSNumber(value: 0o600)]) else {
                throw MacOSProfileRoot.RootError.unavailable
            }
        } else {
            try fileManager.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)], ofItemAtPath: url.path)
        }
    }

    func record(_ event: Event) {
        let line = "\(formatter.string(from: Date())) \(event.rawValue)\n"
        guard let data = line.data(using: .utf8) else { return }
        lock.lock()
        defer { lock.unlock() }
        guard let handle = try? FileHandle(forWritingTo: url) else { return }
        defer { _ = try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: data)
    }
}
