#if os(macOS)
import Foundation

/// macOS seed custody adapter. It delegates every operation to the existing
/// Secure Enclave and Keychain wrapping implementation; it never keeps a seed
/// or writes seed bytes to app storage.
public final class MacOSKeychainSeedProvider: SeedVault {
    private let vault: HardwareSeedVault

    public init() {
        vault = HardwareSeedVault()
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
