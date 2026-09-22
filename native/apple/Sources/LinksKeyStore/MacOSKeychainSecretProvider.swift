#if os(macOS)
import CryptoKit
import Foundation
import Security

/// Profile-scoped secret storage for Rust pre-key material.
///
/// Each value is wrapped by one profile-scoped Secure Enclave P-256 key. The
/// profile state file stores only encrypted Rust state; pre-key seeds never
/// enter it. One wrapping key is important: a pre-key inventory can contain
/// hundreds of seeds, while Secure Enclave key creation is intentionally
/// limited and expensive.
public final class MacOSKeychainSecretProvider: @unchecked Sendable {
    public enum SecretError: Error {
        case invalidInput
        case hardwareUnavailable
        case keyUnavailable
        case storageFailure
        case authenticationFailure
    }

    private static let baseService = "ai.links.desktop.secret.v1"
    private let profile: ClientProfile
    private let namespace: String?
    private let service: String
    private let algorithm = SecKeyAlgorithm.eciesEncryptionCofactorX963SHA256AESGCM
    private let wrappingAccount = "__profile-wrapping-key__"

    public init(profile: ClientProfile = .default, keychainNamespace: String? = nil) {
        self.profile = profile
        self.namespace = keychainNamespace
        if let keychainNamespace {
            service = "\(Self.baseService).\(profile.name).\(keychainNamespace)"
        } else {
            service = profile == .default
                ? Self.baseService
                : "\(Self.baseService).\(profile.name)"
        }
    }

    public func store(_ secret: Data, for key: String) throws {
        guard !key.isEmpty, key.utf8.count <= 128,
              !secret.isEmpty, secret.count <= 64 * 1024 else {
            throw SecretError.invalidInput
        }
        guard SecureEnclave.isAvailable else { throw SecretError.hardwareUnavailable }
        let account = accountName(for: key)
        let privateKey = try wrappingPrivateKey(createIfMissing: true)
        guard let publicKey = SecKeyCopyPublicKey(privateKey),
              SecKeyIsAlgorithmSupported(publicKey, .encrypt, algorithm) else {
            throw SecretError.hardwareUnavailable
        }
        var error: Unmanaged<CFError>?
        var plaintext = context(for: key)
        plaintext.append(secret)
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        guard let sealed = SecKeyCreateEncryptedData(
            publicKey, algorithm, plaintext as CFData, &error) else {
            throw SecretError.authenticationFailure
        }
        try deleteRecord(account)
        var record = Data("LKS3".utf8)
        record.append(sealed as Data)
        var query = recordQuery(account)
        query[kSecValueData as String] = record
        query[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        guard SecItemAdd(query as CFDictionary, nil) == errSecSuccess else {
            throw SecretError.storageFailure
        }
    }

    public func load(for key: String) throws -> Data {
        guard !key.isEmpty, key.utf8.count <= 128 else { throw SecretError.invalidInput }
        let account = accountName(for: key)
        var query = recordQuery(account)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let record = item as? Data, record.count < 64 * 1024 + 4096 else {
            throw SecretError.keyUnavailable
        }
        let privateKey: SecKey
        let sealed: Data
        if record.starts(with: Data("LKS3".utf8)) {
            privateKey = try wrappingPrivateKey(createIfMissing: false)
            sealed = Data(record.dropFirst(4))
        } else if record.starts(with: Data("LKS2".utf8)) {
            // Read records written by the old one-key-per-seed provider.
            privateKey = try legacyPrivateKey(account)
            sealed = Data(record.dropFirst(4))
        } else {
            throw SecretError.keyUnavailable
        }
        var error: Unmanaged<CFError>?
        guard let decoded = SecKeyCreateDecryptedData(
            privateKey, algorithm, sealed as CFData, &error) else {
            throw SecretError.authenticationFailure
        }
        var plaintext = decoded as Data
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        let prefix = context(for: key)
        guard plaintext.count > prefix.count, plaintext.starts(with: prefix) else {
            throw SecretError.authenticationFailure
        }
        return Data(plaintext.dropFirst(prefix.count))
    }

    /// Remove every secret and wrapping key stored for this profile.
    public func eraseProfile() {
        let listQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecMatchLimit as String: kSecMatchLimitAll,
            kSecReturnAttributes as String: true
        ]
        var result: CFTypeRef?
        let status = SecItemCopyMatching(listQuery as CFDictionary, &result)
        if status == errSecSuccess, let items = result as? [[String: Any]] {
            for item in items {
                guard let account = item[kSecAttrAccount as String] as? String else { continue }
                try? deleteRecord(account)
                _ = SecItemDelete(legacyPrivateKeyQuery(account) as CFDictionary)
            }
        }
        _ = SecItemDelete([
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service
        ] as CFDictionary)
        _ = SecItemDelete(wrappingPrivateKeyQuery() as CFDictionary)
    }

    public func delete(_ key: String) throws {
        guard !key.isEmpty, key.utf8.count <= 128 else { throw SecretError.invalidInput }
        let account = accountName(for: key)
        try deleteRecord(account)
        // Legacy LKS2 records owned their own key. New LKS3 records share the
        // profile key, which must remain available for the other seeds.
        let keyStatus = SecItemDelete(legacyPrivateKeyQuery(account) as CFDictionary)
        guard [errSecSuccess, errSecItemNotFound].contains(keyStatus) else {
            throw SecretError.storageFailure
        }
    }

    private func accountName(for key: String) -> String {
        let digest = SHA256.hash(data: Data(key.utf8))
        return digest.map { String(format: "%02x", $0) }.joined()
    }

    private func context(for key: String) -> Data {
        if let namespace {
            return Data("links/desktop-secret/v1\0\(profile.name)\0\(namespace)\0\(key)\0".utf8)
        }
        return Data("links/desktop-secret/v1\0\(profile.name)\0\(key)\0".utf8)
    }

    private func tag(for account: String) -> Data {
        Data("\(service).\(account)".utf8)
    }

    private func wrappingPrivateKey(createIfMissing: Bool) throws -> SecKey {
        var query = wrappingPrivateKeyQuery()
        query[kSecReturnRef as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecSuccess, let item, CFGetTypeID(item) == SecKeyGetTypeID() {
            let key = item as! SecKey
            try requireEnclave(key)
            return key
        }
        guard createIfMissing, status == errSecItemNotFound else {
            throw SecretError.keyUnavailable
        }
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(
            nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, .privateKeyUsage, &error) else {
            throw SecretError.hardwareUnavailable
        }
        let attributes: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrApplicationTag as String: tag(for: wrappingAccount),
                kSecAttrAccessControl as String: access
            ]
        ]
        guard let key = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
            throw SecretError.hardwareUnavailable
        }
        try requireEnclave(key)
        return key
    }

    private func legacyPrivateKey(_ account: String) throws -> SecKey {
        var query = legacyPrivateKeyQuery(account)
        query[kSecReturnRef as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let item, CFGetTypeID(item) == SecKeyGetTypeID() else {
            throw SecretError.keyUnavailable
        }
        let key = item as! SecKey
        try requireEnclave(key)
        return key
    }

    private func deleteRecord(_ account: String) throws {
        let status = SecItemDelete(recordQuery(account) as CFDictionary)
        guard [errSecSuccess, errSecItemNotFound].contains(status) else {
            throw SecretError.storageFailure
        }
    }

    private func recordQuery(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: false
        ]
    }

    private func wrappingPrivateKeyQuery() -> [String: Any] {
        legacyPrivateKeyQuery(wrappingAccount)
    }

    private func legacyPrivateKeyQuery(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassKey,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeyClass as String: kSecAttrKeyClassPrivate,
            kSecAttrApplicationTag as String: tag(for: account)
        ]
    }

    private func requireEnclave(_ key: SecKey) throws {
        guard let attributes = SecKeyCopyAttributes(key) as? [String: Any],
              let token = attributes[kSecAttrTokenID as String] as? String,
              token == kSecAttrTokenIDSecureEnclave as String else {
            throw SecretError.hardwareUnavailable
        }
    }
}
#endif
