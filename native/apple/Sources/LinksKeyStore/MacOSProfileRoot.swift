#if os(macOS)
import CryptoKit
import Foundation

/// The filesystem and metadata namespace owned by one macOS client profile.
/// `baseURL` is the root containing all profiles; `url` is the active profile
/// directory below it.
public struct MacOSProfileRoot: Equatable, Hashable, Sendable {
    public enum RootError: Error, Equatable {
        case unavailable
        case invalidPath
        case metadataUnavailable
    }

    public let profile: ClientProfile
    public let url: URL
    public let logsURL: URL
    /// Stable root scope for Keychain records belonging to this profile root.
    public let keychainNamespace: String
    /// A stable, path-scoped UserDefaults suite for public profile metadata.
    public let metadataSuiteName: String

    public init(profile: ClientProfile, baseURL: URL? = nil,
                fileManager: FileManager = .default) throws {
        self.profile = profile
        let base = try baseURL ?? Self.defaultBaseURL(fileManager: fileManager)
        guard base.isFileURL, base.path.hasPrefix("/"), !base.path.isEmpty else {
            throw RootError.invalidPath
        }
        let normalizedBase = base.standardizedFileURL.resolvingSymlinksInPath()
        guard normalizedBase.path != "/" else { throw RootError.invalidPath }

        let profileURL = normalizedBase
            .appendingPathComponent("\(profile.name)", isDirectory: true)
            .standardizedFileURL
        url = profileURL
        logsURL = profileURL.appendingPathComponent("logs", isDirectory: true)
        let scope = Self.pathScope(for: profileURL)
        keychainNamespace = scope
        metadataSuiteName = "ai.links.client.profile.\(scope)"
    }

    public static func defaultBaseURL(fileManager: FileManager = .default) throws -> URL {
        guard let applicationSupport = fileManager.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first else {
            throw RootError.unavailable
        }
        return applicationSupport
            .appendingPathComponent("Links", isDirectory: true)
            .appendingPathComponent("profiles", isDirectory: true)
    }

    private static func pathScope(for profileURL: URL) -> String {
        let digest = SHA256.hash(data: Data(profileURL.path.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }
}
#endif
