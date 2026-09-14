#if os(macOS)
import CryptoKit
import Foundation
import Security

/// Profile-scoped encrypted state for macOS host data.
///
/// The file contains ciphertext only. Its AES-GCM key is kept in Keychain and
/// is never part of UserDefaults, URLs, logs, or the state file itself.
public final class MacOSEncryptedStateStore {
    public enum StateError: Error {
        case unavailable
        case invalidKey
        case invalidState
        case tooLarge
        case keyGenerationFailed
        case keychainFailure
    }

    public static let maximumPlaintextBytes = 64 * 1024 * 1024

    public let profile: ClientProfile
    private let fileManager: FileManager
    private let directoryURL: URL
    private let stateURL: URL
    private let keychainService: String
    private let keychainAccount: String
    private let associatedDataPrefix: String

    public init(profile: ClientProfile = .default,
                namespace: String = "state",
                fileManager: FileManager = .default) throws {
        self.profile = profile
        self.fileManager = fileManager
        guard Self.isValidNamespace(namespace) else { throw StateError.unavailable }
        guard let applicationSupport = fileManager.urls(
            for: .applicationSupportDirectory, in: .userDomainMask).first else {
            throw StateError.unavailable
        }
        directoryURL = applicationSupport
            .appendingPathComponent("Links", isDirectory: true)
            .appendingPathComponent("profiles", isDirectory: true)
            .appendingPathComponent(profile.name, isDirectory: true)
            .appendingPathComponent("state", isDirectory: true)
        stateURL = directoryURL.appendingPathComponent(
            namespace == "state" ? "state-v1.bin" : "\(namespace)-v1.bin",
            isDirectory: false)
        keychainService = namespace == "state"
            ? "ai.links.local-state.v1.\(profile.name)"
            : "ai.links.local-state.v1.\(profile.name).\(namespace)"
        keychainAccount = namespace == "state" ? "state-key" : "state-key.\(namespace)"
        associatedDataPrefix = namespace == "state"
            ? "links/macos-state/v1\0"
            : "links/macos-state/\(namespace)/v1\0"
    }

    /// Return decrypted state, or nil when this profile has no saved state.
    public func read() throws -> Data? {
        guard fileManager.fileExists(atPath: stateURL.path) else { return nil }
        let ciphertext = try Data(contentsOf: stateURL)
        guard !ciphertext.isEmpty,
              ciphertext.count <= Self.maximumPlaintextBytes + 128 else {
            throw StateError.invalidState
        }
        var keyData = try keyData(createIfMissing: false)
        defer { keyData.resetBytes(in: 0..<keyData.count) }
        do {
            let box = try AES.GCM.SealedBox(combined: ciphertext)
            let key = SymmetricKey(data: keyData)
            let plaintext = try AES.GCM.open(
                box,
                using: key,
                authenticating: Data((associatedDataPrefix + profile.name).utf8))
            guard plaintext.count <= Self.maximumPlaintextBytes else {
                throw StateError.tooLarge
            }
            return plaintext
        } catch let error as StateError {
            throw error
        } catch {
            throw StateError.invalidState
        }
    }

    /// Encrypt and atomically replace the profile's state file.
    public func write(_ plaintext: Data) throws {
        guard plaintext.count <= Self.maximumPlaintextBytes else {
            throw StateError.tooLarge
        }
        try fileManager.createDirectory(
            at: directoryURL, withIntermediateDirectories: true, attributes: nil)
        var keyData = try keyData(createIfMissing: true)
        defer { keyData.resetBytes(in: 0..<keyData.count) }
        let key = SymmetricKey(data: keyData)
        let box = try AES.GCM.seal(
            plaintext,
            using: key,
            authenticating: Data((associatedDataPrefix + profile.name).utf8))
        guard let ciphertext = box.combined else { throw StateError.invalidState }
        try ciphertext.write(to: stateURL, options: [.atomic])
        try fileManager.setAttributes(
            [.posixPermissions: NSNumber(value: 0o600)],
            ofItemAtPath: stateURL.path)
    }

    private func keyData(createIfMissing: Bool) throws -> Data {
        var query = keychainQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecSuccess {
            guard let data = item as? Data, data.count == 32 else {
                throw StateError.invalidKey
            }
            return data
        }
        guard createIfMissing, status == errSecItemNotFound else {
            throw StateError.keychainFailure
        }

        var generated = Data(count: 32)
        let randomStatus = generated.withUnsafeMutableBytes { bytes in
            guard let baseAddress = bytes.baseAddress else { return errSecParam }
            return SecRandomCopyBytes(kSecRandomDefault, bytes.count, baseAddress)
        }
        guard randomStatus == errSecSuccess else {
            generated.resetBytes(in: 0..<generated.count)
            throw StateError.keyGenerationFailed
        }
        var addQuery = keychainQuery()
        addQuery[kSecValueData as String] = generated
        addQuery[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
        if addStatus == errSecDuplicateItem {
            generated.resetBytes(in: 0..<generated.count)
            return try keyData(createIfMissing: false)
        }
        guard addStatus == errSecSuccess else {
            generated.resetBytes(in: 0..<generated.count)
            throw StateError.keychainFailure
        }
        return generated
    }

    private func keychainQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: keychainService,
            kSecAttrAccount as String: keychainAccount,
            kSecAttrSynchronizable as String: false
        ]
    }

    private static func isValidNamespace(_ namespace: String) -> Bool {
        guard !namespace.isEmpty, namespace.utf8.count <= 32 else { return false }
        return namespace.unicodeScalars.allSatisfy {
            ($0.value >= 97 && $0.value <= 122)
                || ($0.value >= 48 && $0.value <= 57)
                || $0.value == 45
                || $0.value == 95
        }
    }
}
#endif
