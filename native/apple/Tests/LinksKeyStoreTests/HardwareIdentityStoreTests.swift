import XCTest
import CryptoKit
@testable import LinksKeyStore

// Test fixture only. Production constructors never accept a software vault.
private final class FixtureVault: SeedVault {
    var seeds: [String: Data] = [:]
    var unavailable = false
    var loads = 0
    var stores = 0
    func storeSeed(_ seed: Data) throws -> String {
        stores += 1
        guard !unavailable else { throw HardwareSeedVault.VaultError.hardwareUnavailable }
        let handle = UUID().uuidString.lowercased()
        seeds[handle] = seed
        return handle
    }
    func loadSeed(_ handle: String) throws -> Data {
        loads += 1
        guard !unavailable else { throw HardwareSeedVault.VaultError.hardwareUnavailable }
        guard let seed = seeds[handle] else { throw HardwareSeedVault.VaultError.keyUnavailable }
        return seed
    }
    func deleteSeed(_ handle: String) throws { seeds.removeValue(forKey: handle) }
}
final class HardwareIdentityStoreTests: XCTestCase {
    func testRustBridgeLifecycleAndSignature() throws {
        let vault = FixtureVault()
        let store = HardwareIdentityStore(vault: vault)
        let identity = try store.createIdentity()
        let reopened = HardwareIdentityStore(vault: vault)
        try reopened.validateIdentity(identity)
        let transcript = Data("links/test/v1\0native".utf8)
        let signature = try reopened.sign(identity, transcript: transcript)
        let publicKey = try Curve25519.Signing.PublicKey(rawRepresentation: identity.publicKey)
        XCTAssertTrue(publicKey.isValidSignature(signature, for: transcript))
        XCTAssertFalse(publicKey.isValidSignature(signature, for: Data("tampered".utf8)))
        XCTAssertEqual(vault.loads, 3)
        try store.deleteIdentity(identity)
        try store.deleteIdentity(identity)
        XCTAssertThrowsError(try reopened.sign(identity, transcript: transcript))
        XCTAssertThrowsError(try reopened.validateIdentity(identity))
        XCTAssertEqual(vault.stores, 1)
    }
    func testUnavailableHardwareAndSubstitutedReferenceFailClosed() throws {
        let vault = FixtureVault()
        let store = HardwareIdentityStore(vault: vault)
        vault.unavailable = true
        XCTAssertThrowsError(try store.createIdentity())
        XCTAssertTrue(vault.seeds.isEmpty)
        vault.unavailable = false
        let first = try store.createIdentity()
        let second = try store.createIdentity()
        let substituted = try IdentityKeyReference(handle: second.handle, publicKey: first.publicKey)
        XCTAssertThrowsError(try store.validateIdentity(substituted))
        XCTAssertThrowsError(try store.sign(substituted, transcript: Data()))
        vault.unavailable = true
        XCTAssertThrowsError(try store.sign(first, transcript: Data()))
    }
    func testReferenceAndMessageValidation() throws {
        XCTAssertThrowsError(try IdentityKeyReference(handle: "../key", publicKey: Data(count: 32)))
        XCTAssertThrowsError(try IdentityKeyReference(handle: UUID().uuidString.lowercased(), publicKey: Data(count: 31)))
        let vault = FixtureVault()
        let store = HardwareIdentityStore(vault: vault)
        let identity = try store.createIdentity()
        let loads = vault.loads
        XCTAssertThrowsError(try store.sign(identity, transcript: Data(count: 1024 * 1024 + 1)))
        XCTAssertEqual(vault.loads, loads)
    }
    func testRealHardwareIdentitySigning() throws {
        guard ProcessInfo.processInfo.environment["LINKS_TEST_SECURE_ENCLAVE"] == "1" else {
            throw XCTSkip("Requires entitled physical Secure Enclave test host.")
        }
        let store = HardwareIdentityStore()
        let identity = try store.createIdentity()
        defer { try? store.deleteIdentity(identity) }
        try HardwareIdentityStore().validateIdentity(identity)
        let transcript = Data("links/test/v1\0hardware".utf8)
        let signature = try store.sign(identity, transcript: transcript)
        XCTAssertTrue(try Curve25519.Signing.PublicKey(rawRepresentation: identity.publicKey)
            .isValidSignature(signature, for: transcript))
        try store.deleteIdentity(identity)
        XCTAssertThrowsError(try store.sign(identity, transcript: transcript))
    }
}
