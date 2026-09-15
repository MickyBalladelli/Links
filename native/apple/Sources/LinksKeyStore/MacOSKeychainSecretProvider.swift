#if os(macOS)
import CryptoKit
import Foundation
import Security

/// Profile-scoped secret storage for Rust pre-key material.
///
/// Each value is wrapped by a fresh Secure Enclave P-256 key. The profile
/// state file stores only encrypted Rust state; pre-key seeds never enter it.
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
                kSecAttrApplicationTag as String: tag(for: account),
                kSecAttrAccessControl as String: access
            ]
        ]
        guard let privateKey = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
            throw SecretError.hardwareUnavailable
        }
        do {
            try requireEnclave(privateKey)
            guard let publicKey = SecKeyCopyPublicKey(privateKey),
                  SecKeyIsAlgorithmSupported(publicKey, .encrypt, algorithm) else {
                throw SecretError.hardwareUnavailable
            }
            var plaintext = context(for: key)
            plaintext.append(secret)
            defer { plaintext.resetBytes(in: 0..<plaintext.count) }
            guard let sealed = SecKeyCreateEncryptedData(
                publicKey, algorithm, plaintext as CFData, &error) else {
                throw SecretError.authenticationFailure
            }
            try? delete(key)
            var record = Data("LKS2".utf8)
            record.append(sealed as Data)
            var query = recordQuery(account)
            query[kSecValueData as String] = record
            query[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            guard SecItemAdd(query as CFDictionary, nil) == errSecSuccess else {
                throw SecretError.storageFailure
            }
        } catch {
            try? delete(key)
            throw error
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
              let record = item as? Data,
              record.starts(with: Data("LKS2".utf8)), record.count < 64 * 1024 + 4096 else {
            throw SecretError.keyUnavailable
        }
        var keyQuery = privateKeyQuery(account)
        keyQuery[kSecReturnRef as String] = true
        keyQuery[kSecMatchLimit as String] = kSecMatchLimitOne
        var keyItem: CFTypeRef?
        guard SecItemCopyMatching(keyQuery as CFDictionary, &keyItem) == errSecSuccess,
              let keyItem, CFGetTypeID(keyItem) == SecKeyGetTypeID() else {
            throw SecretError.keyUnavailable
        }
        let privateKey = keyItem as! SecKey
        try requireEnclave(privateKey)
        var error: Unmanaged<CFError>?
        guard let decoded = SecKeyCreateDecryptedData(
            privateKey, algorithm, Data(record.dropFirst(4)) as CFData, &error) else {
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

    public func delete(_ key: String) throws {
        guard !key.isEmpty, key.utf8.count <= 128 else { throw SecretError.invalidInput }
        let account = accountName(for: key)
        let recordStatus = SecItemDelete(recordQuery(account) as CFDictionary)
        let keyStatus = SecItemDelete(privateKeyQuery(account) as CFDictionary)
        guard [errSecSuccess, errSecItemNotFound].contains(recordStatus),
              [errSecSuccess, errSecItemNotFound].contains(keyStatus) else {
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

    private func recordQuery(_ account: String) -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecAttrSynchronizable as String: false
        ]
    }

    private func privateKeyQuery(_ account: String) -> [String: Any] {
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
