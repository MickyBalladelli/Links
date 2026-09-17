import CryptoKit
import Foundation
import LinksClient
import Security

/// Account-scoped Keychain storage for private pre-key material owned by the
/// shared Rust messaging core. Values never enter UserDefaults or app logs.
final class IOSKeychainSecretProvider: @unchecked Sendable {
    enum SecretError: Error {
        case invalidInput
        case storageFailure
        case keyUnavailable
    }

    private let service: String

    init(accountID: String) {
        service = "ai.links.ios.messaging.secrets.v1.\(accountID)"
    }

    func store(_ secret: Data, for key: String) throws {
        guard !key.isEmpty, key.utf8.count <= 128,
              !secret.isEmpty, secret.count <= 64 * 1024 else {
            throw SecretError.invalidInput
        }
        let query = recordQuery(key)
        let attributes: [String: Any] = [
            kSecValueData as String: secret,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        ]
        let updateStatus = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if updateStatus == errSecSuccess { return }
        guard updateStatus == errSecItemNotFound else {
            throw SecretError.storageFailure
        }
        var insert = query
        attributes.forEach { insert[$0.key] = $0.value }
        guard SecItemAdd(insert as CFDictionary, nil) == errSecSuccess else {
            throw SecretError.storageFailure
        }
    }

    func load(for key: String) throws -> Data {
        guard !key.isEmpty, key.utf8.count <= 128 else {
            throw SecretError.invalidInput
        }
        var query = recordQuery(key)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data,
              !data.isEmpty,
              data.count <= 64 * 1024 else {
            throw SecretError.keyUnavailable
        }
        return data
    }

    func delete(_ key: String) throws {
        guard !key.isEmpty, key.utf8.count <= 128 else {
            throw SecretError.invalidInput
        }
        let status = SecItemDelete(recordQuery(key) as CFDictionary)
        guard status == errSecSuccess || status == errSecItemNotFound else {
            throw SecretError.storageFailure
        }
    }

    private func recordQuery(_ key: String) -> [String: Any] {
        let digest = SHA256.hash(data: Data(key.utf8))
        let account = digest.map { String(format: "%02x", $0) }.joined()
        return [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: false
        ]
    }
}

/// Encrypted, account-scoped durable state for MLS, inbox, outbox, and replay
/// cursors. The state file contains ciphertext only; its key stays in Keychain.
final class IOSEncryptedStateStore: @unchecked Sendable {
    enum StateError: Error {
        case unavailable
        case invalidKey
        case invalidState
        case tooLarge
        case keyGenerationFailed
        case keychainFailure
    }

    static let maximumPlaintextBytes = 64 * 1024 * 1024

    private let fileManager: FileManager
    private let directoryURL: URL
    private let stateURL: URL
    private let keychainService: String
    private let associatedData: Data

    init(accountID: String, fileManager: FileManager = .default) throws {
        guard IOSClient.isCanonicalUUID(accountID),
              let applicationSupport = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask).first else {
            throw StateError.unavailable
        }
        self.fileManager = fileManager
        directoryURL = applicationSupport
            .appendingPathComponent("Links", isDirectory: true)
            .appendingPathComponent("Messaging", isDirectory: true)
            .appendingPathComponent(accountID, isDirectory: true)
        stateURL = directoryURL.appendingPathComponent("core-state-v1.bin")
        keychainService = "ai.links.ios.messaging.state.v1.\(accountID)"
        associatedData = Data("links/ios-state/v1\0\(accountID)".utf8)
    }

    func read() throws -> Data? {
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
            let plaintext = try AES.GCM.open(
                box,
                using: SymmetricKey(data: keyData),
                authenticating: associatedData)
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

    func write(_ plaintext: Data) throws {
        guard plaintext.count <= Self.maximumPlaintextBytes else {
            throw StateError.tooLarge
        }
        try fileManager.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true,
            attributes: [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication])
        var keyData = try keyData(createIfMissing: true)
        defer { keyData.resetBytes(in: 0..<keyData.count) }
        let sealed = try AES.GCM.seal(
            plaintext,
            using: SymmetricKey(data: keyData),
            authenticating: associatedData)
        guard let ciphertext = sealed.combined else { throw StateError.invalidState }
        try ciphertext.write(to: stateURL, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
    }

    private func keyData(createIfMissing: Bool) throws -> Data {
        var query = keychainQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &result)
        if status == errSecSuccess {
            guard let data = result as? Data, data.count == 32 else {
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
        var insert = keychainQuery()
        insert[kSecValueData as String] = generated
        insert[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let addStatus = SecItemAdd(insert as CFDictionary, nil)
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
            kSecAttrAccount as String: "core-state-key",
            kSecAttrSynchronizable as String: false
        ]
    }
}
