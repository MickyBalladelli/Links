import Foundation
import Security
import CryptoKit

/// Ed25519 seed custody, not Ed25519 operations inside Secure Enclave.
/// Uses Apple's ECIES implementation and an enclave-generated P-256 wrapping key.
/// Never substitutes a software key when hardware or Keychain access fails.
public final class HardwareSeedVault {
    public enum VaultError: Error {
        case invalidInput, hardwareUnavailable, keyUnavailable, storageFailure, authenticationFailure
        case keyCreationFailed(Int) // Sanitized OS status, never key material.
    }
    private static let baseService = "ai.links.identity.seed.v1"
    public let profile: ClientProfile
    private let service: String
    private let algorithm = SecKeyAlgorithm.eciesEncryptionCofactorX963SHA256AESGCM
    public init(profile: ClientProfile = .default) {
        self.profile = profile
        service = profile == .default
            ? Self.baseService
            : "\(Self.baseService).\(profile.name)"
    }

    /// Caller generates a random 32-byte seed and wipes its buffers after wrapping.
    public func storeSeed(_ seed: Data) throws -> String {
        guard seed.count == 32 else { throw VaultError.invalidInput }
        guard SecureEnclave.isAvailable else { throw VaultError.hardwareUnavailable }
        let handle = UUID().uuidString.lowercased()
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, .privateKeyUsage, &error) else {
            throw VaultError.hardwareUnavailable
        }
        let attributes: [String: Any] = [
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeySizeInBits as String: 256,
            kSecAttrTokenID as String: kSecAttrTokenIDSecureEnclave,
            kSecPrivateKeyAttrs as String: [
                kSecAttrIsPermanent as String: true,
                kSecAttrApplicationTag as String: try tag(handle),
                kSecAttrAccessControl as String: access
            ]
        ]
        guard let privateKey = SecKeyCreateRandomKey(attributes as CFDictionary, &error) else {
            let code = error.map { CFErrorGetCode($0.takeRetainedValue()) } ?? Int(errSecNotAvailable)
            throw VaultError.keyCreationFailed(code)
        }
        do {
            try requireEnclave(privateKey)
            guard let publicKey = SecKeyCopyPublicKey(privateKey), SecKeyIsAlgorithmSupported(publicKey, .encrypt, algorithm) else {
                throw VaultError.hardwareUnavailable
            }
            var plaintext = context(handle)
            plaintext.append(seed)
            defer { plaintext.resetBytes(in: 0..<plaintext.count) }
            guard let sealed = SecKeyCreateEncryptedData(publicKey, algorithm, plaintext as CFData, &error) else {
                throw VaultError.authenticationFailure
            }
            var record = Data("LKS1".utf8)
            record.append(sealed as Data)
            var query = try recordQuery(handle)
            query[kSecValueData as String] = record
            query[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
            guard SecItemAdd(query as CFDictionary, nil) == errSecSuccess else { throw VaultError.storageFailure }
            return handle
        } catch {
            try? deleteSeed(handle)
            throw error
        }
    }

    /// Returned seed briefly exists in app memory for Ed25519 signing. Caller must
    /// minimize copies and wipe it. A missing/invalidated key requires re-pairing.
    public func loadSeed(_ handle: String) throws -> Data {
        var query = try recordQuery(handle)
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let record = item as? Data, record.starts(with: Data("LKS1".utf8)), record.count < 4096 else {
            throw VaultError.keyUnavailable
        }
        var keyQuery = try privateKeyQuery(handle)
        keyQuery[kSecReturnRef as String] = true
        keyQuery[kSecMatchLimit as String] = kSecMatchLimitOne
        var keyItem: CFTypeRef?
        guard SecItemCopyMatching(keyQuery as CFDictionary, &keyItem) == errSecSuccess, let keyItem else {
            throw VaultError.keyUnavailable
        }
        guard CFGetTypeID(keyItem) == SecKeyGetTypeID() else { throw VaultError.keyUnavailable }
        let key = keyItem as! SecKey
        try requireEnclave(key)
        var error: Unmanaged<CFError>?
        guard let decoded = SecKeyCreateDecryptedData(key, algorithm, Data(record.dropFirst(4)) as CFData, &error) else {
            throw VaultError.authenticationFailure
        }
        var plaintext = decoded as Data
        defer { plaintext.resetBytes(in: 0..<plaintext.count) }
        let prefix = context(handle)
        guard plaintext.count == prefix.count + 32, plaintext.starts(with: prefix) else { throw VaultError.authenticationFailure }
        return Data(plaintext.suffix(32))
    }

    public func deleteSeed(_ handle: String) throws {
        let recordStatus = SecItemDelete(try recordQuery(handle) as CFDictionary)
        let keyStatus = SecItemDelete(try privateKeyQuery(handle) as CFDictionary)
        guard [errSecSuccess, errSecItemNotFound].contains(recordStatus),
              [errSecSuccess, errSecItemNotFound].contains(keyStatus) else { throw VaultError.storageFailure }
    }
    private func requireEnclave(_ key: SecKey) throws {
        guard let attributes = SecKeyCopyAttributes(key) as? [String: Any],
              let token = attributes[kSecAttrTokenID as String] as? String,
              token == kSecAttrTokenIDSecureEnclave as String else { throw VaultError.hardwareUnavailable }
    }
    private func tag(_ handle: String) throws -> Data {
        guard let id = UUID(uuidString: handle), id.uuidString.lowercased() == handle else { throw VaultError.invalidInput }
        return Data("\(service).\(handle)".utf8)
    }
    private func context(_ handle: String) -> Data {
        if profile == .default {
            return Data("links/seed/v1\0\(handle)".utf8)
        }
        return Data("links/seed/v1\0\(profile.name)\0\(handle)".utf8)
    }
    private func recordQuery(_ handle: String) throws -> [String: Any] {
        _ = try tag(handle)
        return [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
                kSecAttrAccount as String: handle, kSecAttrSynchronizable as String: false]
    }
    private func privateKeyQuery(_ handle: String) throws -> [String: Any] {
        [kSecClass as String: kSecClassKey, kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
         kSecAttrKeyClass as String: kSecAttrKeyClassPrivate, kSecAttrApplicationTag as String: try tag(handle)]
    }
}
