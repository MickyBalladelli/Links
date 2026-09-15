import Foundation
import CLinksIdentity

// Internal only: production always uses HardwareSeedVault, never a software vault.
protocol SeedVault: AnyObject {
    func storeSeed(_ seed: Data) throws -> String
    func loadSeed(_ handle: String) throws -> Data
    func deleteSeed(_ handle: String) throws
}
extension HardwareSeedVault: SeedVault {}

/// Public data only. Persist alongside the authenticated account/device binding.
public struct IdentityKeyReference: Equatable {
    public let handle: String
    public let publicKey: Data
    public init(handle: String, publicKey: Data) throws {
        guard let uuid = UUID(uuidString: handle), uuid.uuidString.lowercased() == handle,
              publicKey.count == 32 else { throw HardwareIdentityStore.IdentityError.invalidInput }
        self.handle = handle
        self.publicKey = publicKey
    }
}

/// Rust Ed25519 operations backed by Secure Enclave seed custody, not enclave signing.
/// Run off the main thread. No identity seed is retained between calls.
public final class HardwareIdentityStore {
    public enum IdentityError: Error { case invalidInput, hardwareUnavailable, authenticationFailed, providerFailure }
    private let vault: SeedVault
    private let workerLock = NSLock()
    public let profile: ClientProfile
    public let keychainNamespace: String?
    public init() {
#if os(macOS)
        let provider = MacOSKeychainSeedProvider()
        vault = provider
        profile = provider.profile
        keychainNamespace = provider.keychainNamespace
#else
        vault = HardwareSeedVault()
        profile = .default
        keychainNamespace = nil
#endif
    }
#if os(macOS)
    /// Use the explicit macOS Keychain provider at the host boundary.
    public init(seedProvider: MacOSKeychainSeedProvider) {
        vault = seedProvider
        profile = seedProvider.profile
        keychainNamespace = seedProvider.keychainNamespace
    }
#endif
    internal init(vault: SeedVault) {
        self.vault = vault
        if let provider = vault as? HardwareSeedVault {
            profile = provider.profile
            keychainNamespace = provider.keychainNamespace
        } else {
            profile = .default
            keychainNamespace = nil
        }
    }

    private func withVault<T>(_ operation: (UnsafePointer<LinksVaultCallbacks>) throws -> T) rethrows -> T {
        workerLock.lock()
        defer { workerLock.unlock() }
        // A box keeps the existential and context alive for synchronous callbacks.
        let box = VaultBox(vault)
        var callbacks = LinksVaultCallbacks(
            abi_version: 1, context: Unmanaged.passUnretained(box).toOpaque(),
            store: storeCallback, load: loadCallback, delete_seed: deleteCallback)
        return try withExtendedLifetime(box) { try withUnsafePointer(to: &callbacks, operation) }
    }
    public func createIdentity() throws -> IdentityKeyReference {
        var handle = [UInt8](repeating: 0, count: 36)
        var publicKey = [UInt8](repeating: 0, count: 32)
        try withVault { callbacks in
            try check(links_identity_create(callbacks, &handle, &publicKey))
        }
        return try IdentityKeyReference(handle: String(decoding: handle, as: UTF8.self), publicKey: Data(publicKey))
    }

    /// Derive and hardware-seal an identity from an explicit local BIP-39 phrase.
    public func restoreFromRecovery(_ phrase: String, passphrase: String) throws -> IdentityKeyReference {
        var phraseBytes = Array(phrase.utf8)
        var passphraseBytes = Array(passphrase.utf8)
        defer {
            wipe(&phraseBytes)
            wipe(&passphraseBytes)
        }
        guard !phraseBytes.isEmpty,
              phraseBytes.count <= Int(LINKS_IDENTITY_MAX_RECOVERY_PHRASE),
              passphraseBytes.count <= Int(LINKS_IDENTITY_MAX_RECOVERY_PASSPHRASE) else {
            throw IdentityError.invalidInput
        }
        var handle = [UInt8](repeating: 0, count: 36)
        var publicKey = [UInt8](repeating: 0, count: 32)
        try withVault { callbacks in
            try phraseBytes.withUnsafeBufferPointer { phraseBuffer in
                try passphraseBytes.withUnsafeBufferPointer { passphraseBuffer in
                    try check(links_identity_restore_from_mnemonic(
                        callbacks,
                        phraseBuffer.baseAddress, phraseBuffer.count,
                        passphraseBuffer.baseAddress, passphraseBuffer.count,
                        &handle, &publicKey))
                }
            }
        }
        return try IdentityKeyReference(
            handle: String(decoding: handle, as: UTF8.self), publicKey: Data(publicKey))
    }

    /// Stable WebAuthn PRF salt for passkey-derived identities.
    public func passkeyIdentityPRFSalt() throws -> Data {
        var salt = [UInt8](repeating: 0, count: 32)
        defer { wipe(&salt) }
        try check(links_identity_passkey_prf_salt(&salt))
        return Data(salt)
    }

    /// Generate a local English 12- or 24-word recovery phrase.
    public func generateRecoveryMnemonic(wordCount: Int) throws -> String {
        guard wordCount == 12 || wordCount == 24 else { throw IdentityError.invalidInput }
        var output = [UInt8](repeating: 0, count: Int(LINKS_IDENTITY_MAX_RECOVERY_PHRASE))
        var outputLength = 0
        let status = links_identity_generate_recovery_mnemonic(
            UInt32(wordCount), &output, output.count, &outputLength)
        defer { wipe(&output) }
        try check(status)
        guard outputLength > 0, outputLength <= output.count else {
            throw IdentityError.providerFailure
        }
        return String(decoding: output[0..<outputLength], as: UTF8.self)
    }

    /// Derive and hardware-seal a new identity from a local passkey PRF result.
    public func createFromPasskeyPRF(_ prfOutput: Data) throws -> IdentityKeyReference {
        guard prfOutput.count == 32 else { throw IdentityError.invalidInput }
        var handle = [UInt8](repeating: 0, count: 36)
        var publicKey = [UInt8](repeating: 0, count: 32)
        var prf = prfOutput
        defer { prf.resetBytes(in: 0..<prf.count) }
        try withVault { callbacks in
            try prf.withUnsafeBytes { prfBuffer in
                try check(links_identity_create_from_passkey_prf(
                    callbacks, prfBuffer.bindMemory(to: UInt8.self).baseAddress,
                    &handle, &publicKey))
            }
        }
        return try IdentityKeyReference(
            handle: String(decoding: handle, as: UTF8.self), publicKey: Data(publicKey))
    }
    /// Check a saved reference after restart. A failure never creates a replacement.
    public func validateIdentity(_ identity: IdentityKeyReference) throws {
        let handle = Array(identity.handle.utf8)
        var publicKey = [UInt8](repeating: 0, count: 32)
        try withVault { callbacks in
            try check(links_identity_public_key(callbacks, handle, &publicKey))
        }
        guard Data(publicKey) == identity.publicKey else { throw IdentityError.authenticationFailed }
    }
    public func sign(_ identity: IdentityKeyReference, transcript: Data) throws -> Data {
        guard transcript.count <= LINKS_IDENTITY_MAX_MESSAGE else { throw IdentityError.invalidInput }
        let handle = Array(identity.handle.utf8)
        let publicKey = Array(identity.publicKey)
        var signature = [UInt8](repeating: 0, count: 64)
        try withVault { callbacks in
            try transcript.withUnsafeBytes { message in
                try check(links_identity_sign(callbacks, handle, publicKey,
                    message.bindMemory(to: UInt8.self).baseAddress, message.count, &signature))
            }
        }
        return Data(signature)
    }

    /// Build the exact signed phone-auth transcript used by AccountAuth.
    public func phoneAuthTranscript(_ identity: IdentityKeyReference, phone: String,
                                    channel: String, deviceID: UUID,
                                    mlsNodeID: UUID) throws -> Data {
        let phoneBytes = Array(phone.utf8)
        let channelBytes = Array(channel.utf8)
        let deviceBytes = uuidBytes(deviceID)
        let nodeBytes = uuidBytes(mlsNodeID)
        let publicKey = Array(identity.publicKey)
        guard !phoneBytes.isEmpty, phoneBytes.count <= 16,
              !channelBytes.isEmpty, channelBytes.count <= 8 else {
            throw IdentityError.invalidInput
        }
        var output = [UInt8](repeating: 0, count: Int(LINKS_IDENTITY_MAX_TRANSCRIPT))
        var outputLength = 0
        let status = phoneBytes.withUnsafeBufferPointer { phoneBuffer in
            channelBytes.withUnsafeBufferPointer { channelBuffer in
                deviceBytes.withUnsafeBufferPointer { deviceBuffer in
                    nodeBytes.withUnsafeBufferPointer { nodeBuffer in
                        publicKey.withUnsafeBufferPointer { publicBuffer in
                            output.withUnsafeMutableBufferPointer { outputBuffer in
                                links_phone_auth_transcript(
                                    phoneBuffer.baseAddress, phoneBuffer.count,
                                    channelBuffer.baseAddress, channelBuffer.count,
                                    deviceBuffer.baseAddress, nodeBuffer.baseAddress,
                                    publicBuffer.baseAddress, outputBuffer.baseAddress,
                                    outputBuffer.count, &outputLength)
                            }
                        }
                    }
                }
            }
        }
        return try transcript(status: status, output: output, length: outputLength)
    }

    /// Build the exact nonce-bound enrollment transcript used by AccountAuth.
    public func enrollmentTranscript(_ identity: IdentityKeyReference, userID: UUID,
                                     deviceID: UUID, mlsNodeID: UUID,
                                     challengeID: UUID, nonce: Data,
                                     expiresAtMs: UInt64, mlsCredential: Data) throws -> Data {
        guard nonce.count == 32, mlsCredential.count <= Int(LINKS_IDENTITY_MAX_TRANSCRIPT) else {
            throw IdentityError.invalidInput
        }
        let userBytes = uuidBytes(userID)
        let deviceBytes = uuidBytes(deviceID)
        let nodeBytes = uuidBytes(mlsNodeID)
        let challengeBytes = uuidBytes(challengeID)
        let publicKey = Array(identity.publicKey)
        let nonceBytes = Array(nonce)
        let credentialBytes = Array(mlsCredential)
        var output = [UInt8](repeating: 0, count: Int(LINKS_IDENTITY_MAX_TRANSCRIPT))
        var outputLength = 0
        let status = userBytes.withUnsafeBufferPointer { userBuffer in
            deviceBytes.withUnsafeBufferPointer { deviceBuffer in
                nodeBytes.withUnsafeBufferPointer { nodeBuffer in
                    publicKey.withUnsafeBufferPointer { publicBuffer in
                        challengeBytes.withUnsafeBufferPointer { challengeBuffer in
                            nonceBytes.withUnsafeBufferPointer { nonceBuffer in
                                credentialBytes.withUnsafeBufferPointer { credentialBuffer in
                                    output.withUnsafeMutableBufferPointer { outputBuffer in
                                        links_enrollment_transcript(
                                            userBuffer.baseAddress, deviceBuffer.baseAddress,
                                            nodeBuffer.baseAddress, publicBuffer.baseAddress,
                                            challengeBuffer.baseAddress, nonceBuffer.baseAddress,
                                            expiresAtMs, credentialBuffer.baseAddress,
                                            credentialBuffer.count, outputBuffer.baseAddress,
                                            outputBuffer.count, &outputLength)
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        return try transcript(status: status, output: output, length: outputLength)
    }

    /// Explicit device removal only; never call on a network retry or ordinary login.
    public func deleteIdentity(_ identity: IdentityKeyReference) throws {
        let handle = Array(identity.handle.utf8)
        try withVault { try check(links_identity_delete($0, handle)) }
    }

    private func transcript(status: Int32, output: [UInt8], length: Int) throws -> Data {
        try check(status)
        guard length > 0, length <= output.count else { throw IdentityError.providerFailure }
        return Data(output[0..<length])
    }

    private func uuidBytes(_ value: UUID) -> [UInt8] {
        var tuple = value.uuid
        return withUnsafeBytes(of: &tuple) { Array($0) }
    }

    private func check(_ status: Int32) throws {
        switch status {
        case Int32(LINKS_OK): return
        case Int32(LINKS_INVALID): throw IdentityError.invalidInput
        case Int32(LINKS_UNAVAILABLE): throw IdentityError.hardwareUnavailable
        case Int32(LINKS_AUTHENTICATION): throw IdentityError.authenticationFailed
        default: throw IdentityError.providerFailure
        }
    }
}

private final class VaultBox {
    let vault: SeedVault
    init(_ vault: SeedVault) { self.vault = vault }
}
private func vault(_ context: UnsafeMutableRawPointer?) -> SeedVault {
    Unmanaged<VaultBox>.fromOpaque(context!).takeUnretainedValue().vault
}
private func handleString(_ bytes: UnsafePointer<UInt8>) -> String {
    String(decoding: UnsafeBufferPointer(start: bytes, count: 36), as: UTF8.self)
}
private func wipe(_ bytes: inout [UInt8]) {
    for index in bytes.indices { bytes[index] = 0 }
}
private func vaultStatus(_ error: Error) -> Int32 {
    guard let error = error as? HardwareSeedVault.VaultError else { return Int32(LINKS_PROVIDER) }
    switch error {
    case .invalidInput: return Int32(LINKS_INVALID)
    case .hardwareUnavailable, .keyCreationFailed: return Int32(LINKS_UNAVAILABLE)
    case .keyUnavailable, .authenticationFailure: return Int32(LINKS_AUTHENTICATION)
    case .storageFailure: return Int32(LINKS_PROVIDER)
    }
}
private let storeCallback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UnsafeMutablePointer<UInt8>?) -> Int32 = { context, seed, output in
    guard let context, let seed, let output else { return Int32(LINKS_INVALID) }
    var bytes = Data(bytes: seed, count: 32)
    defer { bytes.resetBytes(in: 0..<bytes.count) }
    do {
        let store = vault(context)
        let handle = try store.storeSeed(bytes)
        guard let uuid = UUID(uuidString: handle), uuid.uuidString.lowercased() == handle else {
            try? store.deleteSeed(handle)
            return Int32(LINKS_PROVIDER)
        }
        Array(handle.utf8).withUnsafeBufferPointer { output.update(from: $0.baseAddress!, count: 36) }
        return Int32(LINKS_OK)
    } catch { return vaultStatus(error) }
}
private let loadCallback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UnsafeMutablePointer<UInt8>?) -> Int32 = { context, handle, output in
    guard let context, let handle, let output else { return Int32(LINKS_INVALID) }
    do {
        var bytes = try vault(context).loadSeed(handleString(handle))
        defer { bytes.resetBytes(in: 0..<bytes.count) }
        guard bytes.count == 32 else { return Int32(LINKS_AUTHENTICATION) }
        bytes.withUnsafeBytes { output.update(from: $0.bindMemory(to: UInt8.self).baseAddress!, count: 32) }
        return Int32(LINKS_OK)
    } catch { return vaultStatus(error) }
}
private let deleteCallback: @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?) -> Int32 = { context, handle in
    guard let context, let handle else { return Int32(LINKS_INVALID) }
    do {
        try vault(context).deleteSeed(handleString(handle))
        return Int32(LINKS_OK)
    } catch { return vaultStatus(error) }
}
