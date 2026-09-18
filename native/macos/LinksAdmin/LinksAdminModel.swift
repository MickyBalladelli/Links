import Foundation
import SwiftUI

struct LinksAdminDevice: Identifiable, Decodable, Equatable {
    let deviceID: UUID
    let registeredAt: String
    let revokedAt: String?

    var id: UUID { deviceID }
    var isActive: Bool { revokedAt == nil }

    private enum CodingKeys: String, CodingKey {
        case deviceID = "device_id"
        case registeredAt = "registered_at"
        case revokedAt = "revoked_at"
    }
}

struct LinksAdminUser: Identifiable, Decodable, Equatable {
    let userID: UUID
    let handle: String?
    let accountKind: String
    let createdAt: String
    let disabledAt: String?
    let devices: [LinksAdminDevice]

    var id: UUID { userID }
    var displayName: String { handle.map { "@\($0)" } ?? userID.uuidString }
    var isDisabled: Bool { disabledAt != nil }
    var activeDeviceCount: Int { devices.filter(\.isActive).count }

    private enum CodingKeys: String, CodingKey {
        case userID = "user_id"
        case handle
        case accountKind = "account_kind"
        case createdAt = "created_at"
        case disabledAt = "disabled_at"
        case devices
    }
}

private enum LinksAdminError: LocalizedError {
    case invalidEndpoint
    case invalidResponse
    case server(String)

    var errorDescription: String? {
        switch self {
        case .invalidEndpoint:
            return "Enter a valid account service URL."
        case .invalidResponse:
            return "The account service returned an invalid response."
        case .server(let message):
            return message
        }
    }
}

@MainActor
final class LinksAdminModel: ObservableObject {
    @Published var endpointText = "http://127.0.0.1:8080"
    @Published var adminKey = ""
    @Published var saveAdminKey = true
    @Published var searchText = ""
    @Published var selectedUserID: UUID?
    @Published private(set) var users: [LinksAdminUser] = []
    @Published private(set) var isLoading = false
    @Published private(set) var isConnected = false
    @Published var errorMessage: String?

    init() {
        if let savedKey = try? LinksAdminKeychain.load() {
            adminKey = savedKey
        }
    }

    var filteredUsers: [LinksAdminUser] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !query.isEmpty else { return users }
        return users.filter { user in
            user.handle?.localizedCaseInsensitiveContains(query) == true
                || user.userID.uuidString.localizedCaseInsensitiveContains(query)
        }
    }

    var selectedUser: LinksAdminUser? {
        guard let selectedUserID else { return nil }
        return users.first { $0.id == selectedUserID }
    }

    func connect() {
        Task { await refresh() }
    }

    func refresh() async {
        guard !adminKey.isEmpty else {
            errorMessage = "Enter the admin key."
            isConnected = false
            return
        }
        isLoading = true
        errorMessage = nil
        do {
            let data = try await request(
                path: ["v1", "admin", "users"],
                queryItems: [URLQueryItem(name: "limit", value: "200")])
            users = try JSONDecoder().decode([LinksAdminUser].self, from: data)
            if selectedUserID == nil || !users.contains(where: { $0.id == selectedUserID }) {
                selectedUserID = users.first?.id
            }
            isConnected = true
            if saveAdminKey {
                do {
                    try LinksAdminKeychain.save(adminKey)
                } catch {
                    errorMessage = error.localizedDescription
                }
            }
        } catch {
            isConnected = false
            errorMessage = error.localizedDescription
        }
        isLoading = false
    }

    func setUserDisabled(_ user: LinksAdminUser, disabled: Bool) async {
        do {
            let body = try JSONEncoder().encode(["disabled": disabled])
            _ = try await request(
                path: ["v1", "admin", "users", user.userID.uuidString, "status"],
                method: "PUT",
                body: body)
            await refresh()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func deleteUser(_ user: LinksAdminUser) async {
        do {
            _ = try await request(
                path: ["v1", "admin", "users", user.userID.uuidString],
                method: "DELETE")
            await refresh()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func revokeDevice(_ device: LinksAdminDevice, for user: LinksAdminUser) async {
        do {
            _ = try await request(
                path: ["v1", "admin", "users", user.userID.uuidString, "devices", device.deviceID.uuidString],
                method: "DELETE")
            await refresh()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func request(
        path: [String],
        queryItems: [URLQueryItem] = [],
        method: String = "GET",
        body: Data? = nil
    ) async throws -> Data {
        guard let baseURL = URL(string: endpointText.trimmingCharacters(in: .whitespacesAndNewlines)),
              baseURL.scheme != nil,
              baseURL.host != nil else {
            throw LinksAdminError.invalidEndpoint
        }
        var url = path.reduce(baseURL) { $0.appendingPathComponent($1) }
        if !queryItems.isEmpty {
            guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else {
                throw LinksAdminError.invalidEndpoint
            }
            components.queryItems = queryItems
            guard let updatedURL = components.url else {
                throw LinksAdminError.invalidEndpoint
            }
            url = updatedURL
        }

        var request = URLRequest(url: url)
        request.httpMethod = method
        request.httpShouldHandleCookies = false
        request.setValue(adminKey, forHTTPHeaderField: "X-Links-Admin-Key")
        if body != nil {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw LinksAdminError.invalidResponse
        }
        guard (200..<300).contains(httpResponse.statusCode) else {
            if httpResponse.statusCode == 401 {
                throw LinksAdminError.server("Admin key was rejected.")
            }
            if httpResponse.statusCode == 503 {
                throw LinksAdminError.server("Admin API is not configured on the account service.")
            }
            throw LinksAdminError.server("Account service error (\(httpResponse.statusCode)).")
        }
        return data
    }
}
