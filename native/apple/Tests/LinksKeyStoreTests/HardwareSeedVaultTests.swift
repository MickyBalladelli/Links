import XCTest
import CryptoKit
import Security
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
    func testHardwareTamperAndWrappingKeyLoss() throws {
        guard ProcessInfo.processInfo.environment["LINKS_TEST_SECURE_ENCLAVE"] == "1" else {
            throw XCTSkip("Requires entitled physical Secure Enclave test host.")
        }
        let vault = HardwareSeedVault()
        let handle = try vault.storeSeed(Data(repeating: 7, count: 32))
        defer { try? vault.deleteSeed(handle) }
        let recordQuery: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: "ai.links.identity.seed.v1",
            kSecAttrAccount as String: handle,
            kSecAttrSynchronizable as String: false
        ]
        var readQuery = recordQuery
        readQuery[kSecReturnData as String] = true
        var value: CFTypeRef?
        XCTAssertEqual(SecItemCopyMatching(readQuery as CFDictionary, &value), errSecSuccess)
        let original = try XCTUnwrap(value as? Data)
        var corrupted = original
        corrupted[corrupted.count - 1] ^= 1
        XCTAssertEqual(SecItemUpdate(recordQuery as CFDictionary,
            [kSecValueData as String: corrupted] as CFDictionary), errSecSuccess)
        XCTAssertThrowsError(try vault.loadSeed(handle))
        XCTAssertEqual(SecItemUpdate(recordQuery as CFDictionary,
            [kSecValueData as String: original] as CFDictionary), errSecSuccess)
        let keyQuery: [String: Any] = [
            kSecClass as String: kSecClassKey,
            kSecAttrKeyType as String: kSecAttrKeyTypeECSECPrimeRandom,
            kSecAttrKeyClass as String: kSecAttrKeyClassPrivate,
            kSecAttrApplicationTag as String: Data("ai.links.identity.seed.v1.\(handle)".utf8)
        ]
        XCTAssertEqual(SecItemDelete(keyQuery as CFDictionary), errSecSuccess)
        XCTAssertThrowsError(try vault.loadSeed(handle))
        XCTAssertEqual(SecItemCopyMatching(keyQuery as CFDictionary, nil), errSecItemNotFound)
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
