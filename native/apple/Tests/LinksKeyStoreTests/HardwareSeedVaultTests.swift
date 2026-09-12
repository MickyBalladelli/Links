import XCTest
import CryptoKit
@testable import LinksKeyStore

final class HardwareSeedVaultTests: XCTestCase {
    func testRejectsWrongSeedSize() {
        XCTAssertThrowsError(try HardwareSeedVault().storeSeed(Data(count: 31)))
        XCTAssertThrowsError(try HardwareSeedVault().storeSeed(Data(count: 33)))
    }
    func testRejectsInvalidHandles() {
        let vault = HardwareSeedVault()
        XCTAssertThrowsError(try vault.loadSeed("../other-key"))
        XCTAssertThrowsError(try vault.deleteSeed("not-a-uuid"))
    }
    func testMissingKeyDoesNotRegenerate() {
        XCTAssertThrowsError(try HardwareSeedVault().loadSeed(UUID().uuidString.lowercased()))
    }
    func testHardwareRoundtripAndDeletion() throws {
        guard ProcessInfo.processInfo.environment["LINKS_TEST_SECURE_ENCLAVE"] == "1" else {
            throw XCTSkip("Opt-in real Secure Enclave/Keychain test; run on an entitled physical device.")
        }
        XCTAssertTrue(SecureEnclave.isAvailable)
        let vault = HardwareSeedVault()
        var seed = Data((0..<32).map(UInt8.init))
        defer { seed.resetBytes(in: 0..<seed.count) }
        let handle = try vault.storeSeed(seed)
        defer { try? vault.deleteSeed(handle) }
        var restored = try HardwareSeedVault().loadSeed(handle)
        defer { restored.resetBytes(in: 0..<restored.count) }
        XCTAssertEqual(restored, seed)
        try vault.deleteSeed(handle)
        XCTAssertThrowsError(try vault.loadSeed(handle))
    }
}
