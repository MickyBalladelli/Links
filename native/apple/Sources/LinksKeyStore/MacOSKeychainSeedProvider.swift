#if os(macOS)
import Foundation

/// macOS seed custody adapter. It delegates every operation to the existing
/// Secure Enclave and Keychain wrapping implementation; it never keeps a seed
/// or writes seed bytes to app storage.
public final class MacOSKeychainSeedProvider: SeedVault {
    private let vault: HardwareSeedVault
    public let profile: ClientProfile
    public let keychainNamespace: String?

    public init(profile: ClientProfile = .default, keychainNamespace: String? = nil) {
        self.profile = profile
        self.keychainNamespace = keychainNamespace
        vault = HardwareSeedVault(profile: profile, keychainNamespace: keychainNamespace)
    }

    func storeSeed(_ seed: Data) throws -> String {
        try vault.storeSeed(seed)
    }

    func loadSeed(_ handle: String) throws -> Data {
        try vault.loadSeed(handle)
    }

    func deleteSeed(_ handle: String) throws {
        try vault.deleteSeed(handle)
    }
}
#endif
