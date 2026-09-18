import Foundation
import Security

enum LinksAdminKeychain {
    private static let service = "Links Admin"
    private static let account = "admin-api-key"

    static func load() throws -> String? {
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query(returningData: true) as CFDictionary, &item)
        if status == errSecItemNotFound {
            return nil
        }
        guard status == errSecSuccess,
              let data = item as? Data,
              let value = String(data: data, encoding: .utf8) else {
            throw LinksAdminKeychainError(status: status)
        }
        return value
    }

    static func save(_ value: String) throws {
        let data = Data(value.utf8)
        let itemQuery = query(returningData: false)
        let updateStatus = SecItemUpdate(
            itemQuery as CFDictionary,
            [kSecValueData as String: data] as CFDictionary)

        if updateStatus == errSecItemNotFound {
            var addQuery = itemQuery
            addQuery[kSecValueData as String] = data
            let addStatus = SecItemAdd(addQuery as CFDictionary, nil)
            guard addStatus == errSecSuccess else {
                throw LinksAdminKeychainError(status: addStatus)
            }
        } else if updateStatus != errSecSuccess {
            throw LinksAdminKeychainError(status: updateStatus)
        }
    }

    private static func query(returningData: Bool) -> [String: Any] {
        var query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account
        ]
        if returningData {
            query[kSecReturnData as String] = true
            query[kSecMatchLimit as String] = kSecMatchLimitOne
        }
        return query
    }
}

private struct LinksAdminKeychainError: LocalizedError {
    let status: OSStatus

    var errorDescription: String? {
        "Could not save the admin key in macOS Keychain (status \(status))."
    }
}
