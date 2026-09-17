import Foundation
import Security
@preconcurrency import LinksClient
import LinksKeyStore
import SwiftUI

enum IOSUsernameAction: String, CaseIterable, Identifiable {
    case register
    case login

    var id: Self { self }

    var title: String {
        switch self {
        case .register: return "Register"
        case .login: return "Log in"
        }
    }
}

struct IOSMobileContact: Identifiable, Equatable, Codable {
    let handle: String
    let userID: String
    let deviceCount: Int

    var id: String { userID }
}

struct IOSMobileConversation: Identifiable, Equatable, Codable {
    let id: String
    let handle: String
    let recipientUserID: String
    let deviceCount: Int
    let createdAt: Date
}

private struct IOSMobilePersistedState: Codable {
    let contacts: [IOSMobileContact]
    let conversations: [IOSMobileConversation]
}

private enum IOSMobileLocalStateStore {
    private static let service = "ai.links.ios.conversations.v1"

    static func load(accountID: String) -> Data? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: accountID,
            kSecAttrSynchronizable as String: false,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne
        ]
        var result: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess else {
            return nil
        }
        return result as? Data
    }

    static func save(_ data: Data, accountID: String) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: accountID,
            kSecAttrSynchronizable as String: false
        ]
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        ]
        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            attributes.forEach { insert[$0.key] = $0.value }
            _ = SecItemAdd(insert as CFDictionary, nil)
        }
    }
}

@MainActor
final class IOSMobileAppModel: ObservableObject {
    @Published private(set) var status = "Opening secure identity store"
    @Published private(set) var identityStatus = "No identity enrolled"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device"
    @Published private(set) var isBusy = false
    @Published private(set) var isEnrolled = false
    @Published private(set) var isAuthenticated = false
    @Published private(set) var pairingStatus = "No pairing activity"
    @Published var error: String?
    @Published var username = ""
    @Published var usernameAction: IOSUsernameAction = .register
    @Published var phone = ""
    @Published var verificationCode = ""
    @Published var channel: IOSOTPChannel = .sms
    @Published var pairingInput = ""
    @Published private(set) var contacts: [IOSMobileContact] = []
    @Published private(set) var conversations: [IOSMobileConversation] = []
    @Published private(set) var isCreatingConversation = false
    @Published private(set) var conversationCreationStatus: String?

    let authEndpointText: String
    private let client: IOSClient?
    private let usernameAuthClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private var otpChallenge: IOSOTPChallenge?

    init() {
        let endpointText = Bundle.main.object(forInfoDictionaryKey: "LINKS_AUTH_URL") as? String
            ?? "https://api.links.invalid"
        authEndpointText = endpointText

        var loadedClient: IOSClient?
        var loadedUsernameAuthClient: IOSUsernameAuthClient?
        var loadedOTPClient: IOSOTPClient?
        var initialError: String?
        do {
            guard let endpoint = URL(string: endpointText) else {
                throw IOSUsernameAuthError.invalidEndpoint
            }
            loadedClient = try IOSClient(
                identityStore: HardwareIdentityStore(),
                defaults: .standard)
            let localRootCertificate = (Bundle.main.object(
                forInfoDictionaryKey: "LINKS_LOCAL_CA_CERT_BASE64") as? String)
                .flatMap { Data(base64Encoded: $0) }
            let urlSession = IOSLocalDevelopmentURLSessionDelegate.makeURLSession(
                baseURL: endpoint,
                rootCertificateData: localRootCertificate)
            loadedUsernameAuthClient = try IOSUsernameAuthClient(
                baseURL: endpoint,
                urlSession: urlSession)
            loadedOTPClient = try IOSOTPClient(baseURL: endpoint)
        } catch {
            initialError = "Mobile client could not open its identity store."
        }
        client = loadedClient
        usernameAuthClient = loadedUsernameAuthClient
        otpClient = loadedOTPClient
        error = initialError
        restoreLocalState()
        if initialError != nil {
            status = "Identity store unavailable"
        } else {
            refreshState()
        }
    }

    func createIdentity() {
        createIdentity(completion: nil)
    }

    private func createIdentity(completion: (() -> Void)?) {
        guard let client, !client.isEnrolled, !isBusy else { return }
        isBusy = true
        error = nil
        status = "Creating hardware-backed identity"
        DispatchQueue.global(qos: .userInitiated).async { [weak self, client] in
            do {
                try client.createIdentity()
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.isBusy = false
                    self.status = "Identity ready"
                    self.refreshState()
                    completion?()
                }
            } catch {
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.isBusy = false
                    self.status = "Identity creation failed"
                    self.error = "Secure identity creation failed. Check device security settings."
                }
            }
        }
    }

    func authenticateUsername() {
        guard !isBusy else { return }
        guard let client, let usernameAuthClient else {
            status = "Identity store unavailable"
            error = "Mobile client could not open its identity store."
            return
        }
        let cleanUsername = username.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        do {
            try IOSUsernameAuthClient.validateHandle(cleanUsername)
        } catch {
            self.error = "Use a lowercase username with 3–32 letters, numbers, or underscores."
            return
        }

        let action = usernameAction
        if !client.isEnrolled {
            guard action == .register else {
                status = "Identity required"
                error = "Create or restore your hardware identity before logging in."
                return
            }
            createIdentity { [weak self, client, usernameAuthClient] in
                self?.startUsernameAuthentication(
                    using: client,
                    api: usernameAuthClient,
                    handle: cleanUsername,
                    action: action)
            }
            return
        }

        startUsernameAuthentication(
            using: client,
            api: usernameAuthClient,
            handle: cleanUsername,
            action: action)
    }

    private func startUsernameAuthentication(
        using client: IOSClient,
        api: IOSUsernameAuthClient,
        handle: String,
        action: IOSUsernameAction) {
        isBusy = true
        error = nil
        status = action == .register ? "Registering username" : "Logging in"
        Task { @MainActor [weak self] in
            do {
                switch action {
                case .register:
                    _ = try await client.registerUsername(using: api, handle: handle)
                case .login:
                    _ = try await client.loginUsername(using: api, handle: handle)
                }
                guard let self else { return }
                self.username = ""
                self.isBusy = false
                self.status = action == .register ? "Username registered" : "Account authenticated"
                self.refreshState()
            } catch let usernameError as IOSUsernameAuthError {
                if action == .register && Self.isRegistrationConflict(usernameError) {
                    do {
                        _ = try await client.loginUsername(using: api, handle: handle)
                        guard let self else { return }
                        self.username = ""
                        self.isBusy = false
                        self.status = "Account authenticated"
                        self.refreshState()
                        return
                    } catch {
                        // Keep the original registration error when recovery cannot log in.
                    }
                }
                guard let self else { return }
                self.isBusy = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = self.usernameErrorMessage(usernameError, action: action)
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = "The local identity could not complete the request. Check the profile and auth service."
            }
        }
    }

    private static func isRegistrationConflict(_ error: IOSUsernameAuthError) -> Bool {
        switch error {
        case .conflict, .deviceAlreadyRegistered:
            return true
        default:
            return false
        }
    }

    private func usernameErrorMessage(_ error: IOSUsernameAuthError,
                                      action: IOSUsernameAction) -> String {
        switch error {
        case .invalidEndpoint:
            return "The auth service URL is invalid. Check LINKS_AUTH_URL."
        case .invalidHandle:
            return "Use a lowercase username with 3–32 letters, numbers, or underscores."
        case .invalidRequest:
            return "The auth request was invalid. Recreate the local identity and try again."
        case .invalidResponse:
            return "The auth service returned an invalid response. Check the local backend."
        case .serviceRejected:
            return "Cannot reach the auth service at \(authEndpointText). Check the endpoint and local backend."
        case .networkUnavailable:
            return "This iPhone has no usable network connection."
        case .cannotConnect:
            return "Cannot connect to \(authEndpointText). Check the Mac IP, same Wi-Fi, and HTTPS proxy."
        case .timedOut:
            return "The auth service at \(authEndpointText) timed out. Check the Mac and local backend."
        case .tlsRejected:
            return "The iPhone rejected the HTTPS certificate. Install and fully trust the Links local root certificate."
        case .serverRejected(let statusCode):
            switch statusCode {
            case 400:
                return "The auth service rejected the request (HTTP 400). Check the backend and identity."
            case 401, 403:
                return action == .login
                    ? "The auth service rejected this identity (HTTP \(statusCode)). Use the profile that registered this username."
                    : "The auth service rejected this identity (HTTP \(statusCode))."
            case 404:
                return "The auth endpoint was not found (HTTP 404). Check the auth service URL."
            case 500...599:
                return "The auth service is unavailable (HTTP \(statusCode)). Restart the local backend."
            default:
                return "The auth service rejected the request (HTTP \(statusCode))."
            }
        case .conflict:
            return action == .register
                ? "This username or iPhone is already registered. Switch to Log in with the first username."
                : "Username already exists."
        case .deviceAlreadyRegistered:
            return "This iPhone already has a registered username. Switch to Log in and use the first username."
        case .rateLimited(let retryAfterSeconds):
            if let retryAfterSeconds {
                return "Too many requests. Try again in \(retryAfterSeconds) seconds."
            }
            return "Too many requests. Try again later."
        }
    }

    func sendVerificationCode() {
        guard let client, let otpClient, client.isEnrolled, !isBusy else { return }
        let cleanPhone = phone.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanPhone.isEmpty else {
            error = "Enter a phone number first."
            return
        }
        isBusy = true
        error = nil
        status = "Sending verification code"
        let selectedChannel = channel
        Task { @MainActor [weak self] in
            do {
                let challenge = try await client.startOTP(
                    using: otpClient,
                    phone: cleanPhone,
                    channel: selectedChannel)
                guard let self else { return }
                self.otpChallenge = challenge
                self.isBusy = false
                self.status = "Code sent"
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.status = "Code request failed"
                self.error = "Could not send the verification code."
            }
        }
    }

    func verifyCode() {
        guard let client, let otpClient, let challenge = otpChallenge, !isBusy else { return }
        let code = verificationCode.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty else {
            error = "Enter the verification code first."
            return
        }
        isBusy = true
        error = nil
        status = "Verifying phone"
        Task { @MainActor [weak self] in
            do {
                _ = try await client.finishOTP(using: otpClient, challenge: challenge, code: code)
                guard let self else { return }
                self.otpChallenge = nil
                self.verificationCode = ""
                self.isBusy = false
                self.status = "Account authenticated"
                self.refreshState()
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.status = "Verification failed"
                self.error = "The code was rejected or expired. Request a new code."
            }
        }
    }

    func approvePairing() {
        guard let client, let usernameAuthClient, client.isAuthenticated, !isBusy else { return }
        let uri = pairingInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !uri.isEmpty else {
            error = "Paste a links://connect link first."
            return
        }
        isBusy = true
        error = nil
        pairingStatus = "Checking pairing link"
        Task { @MainActor [weak self] in
            do {
                let response = try await client.approvePairing(using: usernameAuthClient, uri: uri)
                guard let self else { return }
                self.isBusy = false
                self.pairingInput = ""
                self.pairingStatus = "Device approved: \(String(response.deviceID.prefix(8)))"
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.pairingStatus = "Pairing approval failed"
                self.error = "The pairing link was rejected. Check the account and try again."
            }
        }
    }

    func handleIncomingURL(_ url: URL) {
        guard url.scheme?.lowercased() == "links",
              url.host?.lowercased() == "connect",
              url.path.isEmpty || url.path == "/",
              url.query?.isEmpty == false else {
            error = "Invalid Links pairing link."
            return
        }
        pairingInput = url.absoluteString
        pairingStatus = "Pairing link received. Review before approval."
    }

    var profileName: String {
        client?.accountHandle.map { "@\($0)" } ?? "Links user"
    }

    func createConversation(handle: String) async -> IOSMobileConversation? {
        guard let client, let usernameAuthClient, client.isAuthenticated,
              !isCreatingConversation else {
            conversationCreationStatus = "Sign in before starting a conversation."
            return nil
        }
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "^@", with: "", options: .regularExpression)
        do {
            try IOSUsernameAuthClient.validateHandle(cleanHandle)
        } catch {
            conversationCreationStatus = "Use 3–32 lowercase letters, numbers, or underscores."
            return nil
        }

        isCreatingConversation = true
        conversationCreationStatus = "Finding @\(cleanHandle)…"
        defer { isCreatingConversation = false }
        do {
            let directory = try await usernameAuthClient.lookup(handle: cleanHandle)
            guard directory.userID != client.userID else {
                conversationCreationStatus = "Choose someone other than your own account."
                return nil
            }
            guard !directory.devices.isEmpty else {
                conversationCreationStatus = "@\(directory.handle) has no active devices."
                return nil
            }

            let contact = IOSMobileContact(
                handle: directory.handle,
                userID: directory.userID,
                deviceCount: directory.devices.count)
            if let contactIndex = contacts.firstIndex(where: { $0.userID == contact.userID }) {
                contacts[contactIndex] = contact
            } else {
                contacts.append(contact)
                contacts.sort { $0.handle < $1.handle }
            }

            if let existingIndex = conversations.firstIndex(where: {
                $0.recipientUserID == directory.userID
            }) {
                let existing = conversations.remove(at: existingIndex)
                let updated = IOSMobileConversation(
                    id: existing.id,
                    handle: directory.handle,
                    recipientUserID: directory.userID,
                    deviceCount: directory.devices.count,
                    createdAt: existing.createdAt)
                conversations.insert(updated, at: 0)
                conversationCreationStatus = "Opened @\(directory.handle)."
                persistLocalState()
                return updated
            }

            let conversation = IOSMobileConversation(
                id: UUID().uuidString.lowercased(),
                handle: directory.handle,
                recipientUserID: directory.userID,
                deviceCount: directory.devices.count,
                createdAt: Date())
            conversations.insert(conversation, at: 0)
            conversationCreationStatus = "Conversation with @\(directory.handle) is ready."
            persistLocalState()
            return conversation
        } catch let authError as IOSUsernameAuthError {
            switch authError {
            case .serverRejected(let statusCode) where statusCode == 404:
                conversationCreationStatus = "No Links account uses @\(cleanHandle)."
            case .rateLimited:
                conversationCreationStatus = "Too many searches. Wait a moment and try again."
            case .networkUnavailable, .cannotConnect, .timedOut:
                conversationCreationStatus = "The directory is unavailable. Check your connection."
            case .tlsRejected:
                conversationCreationStatus = "The secure connection to the directory was rejected."
            default:
                conversationCreationStatus = "Could not add @\(cleanHandle)."
            }
            return nil
        } catch {
            conversationCreationStatus = "Could not add @\(cleanHandle)."
            return nil
        }
    }

    func removeContact(_ contact: IOSMobileContact) {
        contacts.removeAll { $0.userID == contact.userID }
        persistLocalState()
    }

    func deleteConversation(_ conversation: IOSMobileConversation) {
        conversations.removeAll { $0.id == conversation.id }
        persistLocalState()
    }

    func clearConversationCreationStatus() {
        conversationCreationStatus = nil
    }

    func signOut() {
        client?.clearAuthenticatedSession()
        status = "Signed out"
        error = nil
        conversationCreationStatus = nil
        refreshState()
    }

    func clearError() {
        error = nil
    }

    private func restoreLocalState() {
        guard let accountID = client?.userID,
              let data = IOSMobileLocalStateStore.load(accountID: accountID),
              let state = try? PropertyListDecoder().decode(
                IOSMobilePersistedState.self, from: data) else {
            return
        }
        contacts = state.contacts
        conversations = state.conversations.sorted { $0.createdAt > $1.createdAt }
    }

    private func persistLocalState() {
        guard let accountID = client?.userID else { return }
        let state = IOSMobilePersistedState(
            contacts: contacts,
            conversations: conversations)
        guard let data = try? PropertyListEncoder().encode(state) else { return }
        IOSMobileLocalStateStore.save(data, accountID: accountID)
    }

    private func refreshState() {
        guard let client else {
            isEnrolled = false
            isAuthenticated = false
            identityStatus = "Identity store unavailable"
            accountStatus = "Signed out"
            deviceStatus = "Unavailable"
            return
        }
        isEnrolled = client.isEnrolled
        isAuthenticated = client.isAuthenticated
        identityStatus = client.isEnrolled ? "Hardware identity enrolled" : "No identity enrolled"
        if let userID = client.userID {
            let handle = client.accountHandle.map { "@\($0) · " } ?? ""
            accountStatus = client.isAuthenticated
                ? "\(handle)Authenticated · \(String(userID.prefix(8)))"
                : "\(handle)Account saved · sign in again"
        } else {
            accountStatus = "Signed out"
        }
        if let deviceID = client.deviceID {
            deviceStatus = "Device \(String(deviceID.prefix(8)))"
        } else {
            deviceStatus = "No device"
        }
    }
}

