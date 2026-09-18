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

struct IOSMobileMessage: Identifiable, Equatable, Codable {
    let id: String
    let text: String
    let isOutgoing: Bool
    let sentAt: Date
    let senderDeviceID: String?
}

struct IOSMobileConversation: Identifiable, Equatable, Codable {
    let id: String
    var handle: String
    let recipientUserID: String
    var deviceCount: Int
    let createdAt: Date
    var messages: [IOSMobileMessage]
    var isSecureReady: Bool

    init(id: String, handle: String, recipientUserID: String, deviceCount: Int,
         createdAt: Date, messages: [IOSMobileMessage] = [], isSecureReady: Bool = false) {
        self.id = id
        self.handle = handle
        self.recipientUserID = recipientUserID
        self.deviceCount = deviceCount
        self.createdAt = createdAt
        self.messages = messages
        self.isSecureReady = isSecureReady
    }

    private enum CodingKeys: String, CodingKey {
        case id, handle, recipientUserID, deviceCount, createdAt, messages, isSecureReady
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        handle = try values.decode(String.self, forKey: .handle)
        recipientUserID = try values.decode(String.self, forKey: .recipientUserID)
        deviceCount = try values.decode(Int.self, forKey: .deviceCount)
        createdAt = try values.decode(Date.self, forKey: .createdAt)
        messages = try values.decodeIfPresent([IOSMobileMessage].self, forKey: .messages) ?? []
        isSecureReady = try values.decodeIfPresent(Bool.self, forKey: .isSecureReady) ?? false
    }
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
    private static let manualSignOutKey = "ai.links.ios.manual-sign-out.v1"

    @Published private(set) var status = "Opening secure identity store"
    @Published private(set) var identityStatus = "No identity enrolled"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device"
    @Published private(set) var isBusy = false
    @Published private(set) var isEnrolled = false
    @Published private(set) var isAuthenticated = false
    @Published private(set) var isRestoringSession = false
    @Published private(set) var requiresManualSignIn = true
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
    @Published private(set) var messagingState: IOSDirectMessaging.State = .stopped
    @Published private(set) var messagingStatus = "Offline"
    @Published private(set) var preKeyStatus = "Waiting for sign in"
    @Published private(set) var preparingConversationIDs = Set<String>()
    private var resolvingIncomingUserIDs = Set<String>()

    let authEndpointText: String
    private var client: IOSClient?
    private let usernameAuthClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private var preKeyAPI: IOSPreKeyHTTPClient?
    private var messaging: IOSDirectMessaging?
    private var directChatDirectory: (any IOSDirectChatDirectory)?
    private var coreFactory: IOSRustCoreFactory?
    private var coreStateStore: IOSEncryptedStateStore?
    private var keychainSecretProvider: IOSKeychainSecretProvider?
    private var localRootCertificateData: Data?
    private var otpChallenge: IOSOTPChallenge?

    init() {
        let endpointText = Bundle.main.object(forInfoDictionaryKey: "LINKS_AUTH_URL") as? String
            ?? "https://api.links.invalid"
        authEndpointText = endpointText

        var loadedClient: IOSClient?
        var loadedUsernameAuthClient: IOSUsernameAuthClient?
        var loadedOTPClient: IOSOTPClient?
        var loadedLocalRootCertificateData: Data?
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
            loadedLocalRootCertificateData = localRootCertificate
            let urlSession = IOSLocalDevelopmentURLSessionDelegate.makeURLSession(
                baseURL: endpoint,
                rootCertificateData: localRootCertificate)
            loadedUsernameAuthClient = try IOSUsernameAuthClient(
                baseURL: endpoint,
                urlSession: urlSession)
            loadedOTPClient = try IOSOTPClient(baseURL: endpoint, urlSession: urlSession)
            preKeyAPI = try IOSPreKeyHTTPClient(baseURL: endpoint, urlSession: urlSession)
        } catch {
            initialError = "Mobile client could not open its identity store."
        }
        client = loadedClient
        usernameAuthClient = loadedUsernameAuthClient
        otpClient = loadedOTPClient
        localRootCertificateData = loadedLocalRootCertificateData
        let hasSavedAccount = loadedClient?.accountHandle != nil
        let manuallySignedOut = UserDefaults.standard.bool(forKey: Self.manualSignOutKey)
        requiresManualSignIn = !hasSavedAccount || manuallySignedOut
        if let savedHandle = loadedClient?.accountHandle, manuallySignedOut {
            usernameAction = .login
            username = savedHandle
        }
        if hasSavedAccount && !manuallySignedOut && loadedUsernameAuthClient != nil {
            isRestoringSession = true
        }
        error = initialError
        restoreLocalState()
        if initialError != nil {
            status = "Identity store unavailable"
        } else {
            refreshState()
        }
        if isAuthenticated {
            Task { [weak self] in
                self?.configureMessagingIfPossible()
            }
        } else if isRestoringSession {
            restoreSavedSession()
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

    /// Move to a new local profile without deleting the current account.
    /// Each profile gets its own hardware-backed identity and metadata namespace.
    func enrollNewUsername() {
        guard !isBusy else { return }
        let cleanUsername = username.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        do {
            try IOSUsernameAuthClient.validateHandle(cleanUsername)
            let defaultClient = try IOSClient(
                identityStore: HardwareIdentityStore(),
                defaults: .standard,
                profile: .default)
            let profile = defaultClient.accountHandle == cleanUsername
                ? ClientProfile.default
                : try ClientProfile(name: cleanUsername)
            persistLocalState()
            stopActiveSession()

            let newClient = try IOSClient(
                identityStore: HardwareIdentityStore(profile: profile),
                defaults: .standard,
                profile: profile)
            client = newClient
            contacts.removeAll()
            conversations.removeAll()
            restoreLocalState()
            usernameAction = newClient.accountHandle == nil ? .register : .login
            username = cleanUsername
            error = nil
            status = "Preparing @\(cleanUsername)"
            refreshState()

            if !newClient.isEnrolled {
                createIdentity { [weak self] in
                    self?.authenticateUsername()
                }
            }
        } catch {
            status = "New username setup failed"
            self.error = "Could not create a secure profile for this username."
        }
    }

    private func startUsernameAuthentication(
        using client: IOSClient,
        api: IOSUsernameAuthClient,
        handle: String,
        action: IOSUsernameAction) {
        guard !isBusy else { return }
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
                self.isRestoringSession = false
                self.requiresManualSignIn = false
                UserDefaults.standard.set(false, forKey: Self.manualSignOutKey)
                self.status = action == .register ? "Username registered" : "Account authenticated"
                self.refreshState()
            } catch let usernameError as IOSUsernameAuthError {
                if action == .register && Self.isRegistrationConflict(usernameError) {
                    do {
                        _ = try await client.loginUsername(using: api, handle: handle)
                        guard let self else { return }
                        self.username = ""
                        self.isBusy = false
                        self.isRestoringSession = false
                        self.requiresManualSignIn = false
                        UserDefaults.standard.set(false, forKey: Self.manualSignOutKey)
                        self.status = "Account authenticated"
                        self.refreshState()
                        return
                    } catch {
                        // Keep the original registration error when recovery cannot log in.
                    }
                }
                guard let self else { return }
                self.isBusy = false
                self.isRestoringSession = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = self.usernameErrorMessage(usernameError, action: action)
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.isRestoringSession = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = "The local identity could not complete the request. Check the profile and auth service."
            }
        }
    }

    func restoreSavedSession() {
        guard !requiresManualSignIn,
              !isBusy,
              let client,
              let usernameAuthClient,
              client.isEnrolled,
              let handle = client.accountHandle else {
            isRestoringSession = false
            return
        }
        if messaging != nil {
            stopActiveSession()
        }
        isRestoringSession = true
        startUsernameAuthentication(
            using: client,
            api: usernameAuthClient,
            handle: handle,
            action: .login)
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
            guard let retryAfterSeconds else {
                return "Too many requests. Try again later."
            }
            if retryAfterSeconds >= 3_600 {
                let hours = Int(ceil(Double(retryAfterSeconds) / 3_600))
                return hours == 1
                    ? "Too many requests. Try again in about an hour."
                    : "Too many requests. Try again in about \(hours) hours."
            }
            if retryAfterSeconds >= 60 {
                let minutes = Int(ceil(Double(retryAfterSeconds) / 60))
                return "Too many requests. Try again in about \(minutes) minute\(minutes == 1 ? "" : "s")."
            }
            return "Too many requests. Try again in \(retryAfterSeconds) second\(retryAfterSeconds == 1 ? "" : "s")."
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
                self.requiresManualSignIn = false
                UserDefaults.standard.set(false, forKey: Self.manualSignOutKey)
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
                    createdAt: existing.createdAt,
                    messages: existing.messages,
                    isSecureReady: existing.isSecureReady)
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

    func prepareConversation(_ conversationID: String) async {
        guard let index = conversations.firstIndex(where: { $0.id == conversationID }) else { return }
        if conversations[index].isSecureReady,
           let messaging,
           messaging.hasRecipientDevices(for: conversations[index].recipientUserID) {
            return
        }
        guard preparingConversationIDs.insert(conversationID).inserted else { return }
        defer { preparingConversationIDs.remove(conversationID) }

        configureMessagingIfPossible()
        for _ in 0..<75 where messagingState != .ready || !preKeyStatus.hasPrefix("Ready") {
            if messagingState == .authenticationRequired || messagingState == .failed { break }
            try? await Task.sleep(nanoseconds: 200_000_000)
        }

        guard let messaging, let directChatDirectory, let preKeyAPI,
              messagingState == .ready, preKeyStatus.hasPrefix("Ready") else {
            error = messagingSetupError
            return
        }
        let conversation = conversations[index]
        do {
            try await messaging.initializeFirstDirectConversation(
                conversationID: conversation.id,
                recipientUserID: conversation.recipientUserID,
                directory: directChatDirectory,
                preKeyAPI: preKeyAPI)
            guard let currentIndex = conversations.firstIndex(where: { $0.id == conversationID }) else {
                return
            }
            conversations[currentIndex].isSecureReady = true
            messagingStatus = "End-to-end encrypted"
            error = nil
            persistLocalState()
        } catch {
            self.error = "Secure chat setup failed. The contact must be online with pre-keys available."
        }
    }

    func sendMessage(conversationID: String, text: String) -> Bool {
        let cleanText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanText.isEmpty,
              cleanText.utf8.count <= IOSDirectMessaging.maximumTextBytes,
              let index = conversations.firstIndex(where: { $0.id == conversationID }) else {
            return false
        }
        guard conversations[index].isSecureReady else {
            error = "Wait for secure chat setup to finish before sending."
            return false
        }
        guard let messaging, messagingState == .ready else {
            error = "Messaging is reconnecting. Your text remains in the composer."
            return false
        }
        do {
            try messaging.sendText(
                conversationID: conversationID,
                recipientUserID: conversations[index].recipientUserID,
                text: cleanText)
            conversations[index].messages.append(IOSMobileMessage(
                id: UUID().uuidString.lowercased(),
                text: cleanText,
                isOutgoing: true,
                sentAt: Date(),
                senderDeviceID: nil))
            error = nil
            persistLocalState()
            return true
        } catch {
            self.error = messaging.pendingOutboxCount > 0
                ? "Message is encrypted and queued for delivery."
                : "Message could not be sent. Check the connection and try again."
            return false
        }
    }

    func conversation(withID conversationID: String) -> IOSMobileConversation? {
        conversations.first { $0.id == conversationID }
    }

    private var messagingSetupError: String {
        switch messagingState {
        case .connecting, .reconnecting:
            return "Connecting to secure messaging. Try again in a moment."
        case .authenticationRequired:
            return "Your session expired. Log in again to continue messaging."
        case .staleCursor:
            return "Message history needs secure recovery before continuing."
        case .dependencyOutage, .failed, .sendFailed:
            return "The messaging service is unavailable. Check the local gateway."
        case .stopped:
            return "Secure messaging has not started."
        case .ready:
            return preKeyStatus
        }
    }

    private func configureMessagingIfPossible() {
        guard messaging == nil,
              let client, client.isAuthenticated,
              let accountID = client.userID,
              let usernameAuthClient,
              let preKeyAPI,
              let authURL = URL(string: authEndpointText),
              var components = URLComponents(url: authURL, resolvingAgainstBaseURL: false) else {
            return
        }
        components.scheme = components.scheme?.lowercased() == "https" ? "wss" : "ws"
        components.path = "/v1/connect"
        components.query = nil
        components.fragment = nil
        guard let gatewayURL = components.url else { return }

        do {
            let stateStore = try IOSEncryptedStateStore(accountID: accountID)
            let secrets = IOSKeychainSecretProvider(accountID: accountID)
            let factory = IOSRustCoreFactory(stateStore: stateStore, secrets: secrets)
            let keyPackageProvider = IOSHTTPMLSKeyPackageProvider(api: preKeyAPI)
            let directory = IOSDirectoryChatAdapter(
                directoryClient: usernameAuthClient,
                keyPackageProvider: keyPackageProvider) { [weak self] in
                    guard let self else { return [:] }
                    return Dictionary(uniqueKeysWithValues: self.contacts.map {
                        ($0.userID, $0.handle)
                    })
                }
            let directMessaging = IOSDirectMessaging(
                client: client,
                factory: factory,
                endpoint: gatewayURL,
                delegate: self,
                localDevelopmentRootCertificateData: localRootCertificateData)
            coreStateStore = stateStore
            keychainSecretProvider = secrets
            coreFactory = factory
            directChatDirectory = directory
            messaging = directMessaging
            messagingState = .connecting
            messagingStatus = "Connecting securely"
            preKeyStatus = "Preparing encryption keys"
            // Authenticated sessions connect as soon as the messaging host is ready.
            try directMessaging.start()
            Task { @MainActor [weak self, weak directMessaging] in
                guard let self, let directMessaging else { return }
                do {
                    let inventory = try await directMessaging.maintainPreKeyInventory(using: preKeyAPI)
                    guard self.messaging === directMessaging else { return }
                    self.preKeyStatus = "Ready · \(inventory.oneTimeCurvePreKeys) curve · \(inventory.oneTimeKEMPreKeys) KEM"
                } catch {
                    guard self.messaging === directMessaging else { return }
                    self.preKeyStatus = "Encryption key setup failed"
                    self.error = "Could not publish this device’s encryption keys."
                }
            }
        } catch {
            messagingState = .failed
            messagingStatus = "Secure messaging unavailable"
            self.error = "The encrypted messaging core could not start on this iPhone."
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
        let signingOutClient = client
        let accessToken = signingOutClient.flatMap { try? $0.accessToken() }
        stopActiveSession()
        UserDefaults.standard.set(true, forKey: Self.manualSignOutKey)
        requiresManualSignIn = true
        isRestoringSession = false
        usernameAction = .login
        username = client?.accountHandle ?? ""
        status = "Signing out…"
        error = nil
        conversationCreationStatus = nil

        Task { @MainActor [weak self] in
            var remoteFailure = accessToken != nil
            if let accessToken, let usernameAuthClient = self?.usernameAuthClient {
                do {
                    try await usernameAuthClient.logout(accessToken: accessToken)
                    remoteFailure = false
                } catch {
                    remoteFailure = true
                }
            }
            let currentToken: String?
            if let signingOutClient {
                currentToken = try? signingOutClient.accessToken()
            } else {
                currentToken = nil
            }
            if accessToken == nil || currentToken == accessToken {
                signingOutClient?.clearAuthenticatedSession()
            }
            guard let self else { return }
            self.status = "Signed out"
            self.error = remoteFailure
                ? "Signed out on this iPhone, but remote session revocation could not be confirmed."
                : nil
            self.refreshState()
        }
    }

    private func stopActiveSession() {
        messaging?.shutdown()
        messaging = nil
        directChatDirectory = nil
        coreFactory = nil
        coreStateStore = nil
        keychainSecretProvider = nil
        messagingState = .stopped
        messagingStatus = "Offline"
        preKeyStatus = "Waiting for sign in"
        preparingConversationIDs.removeAll()
        otpChallenge = nil
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
        if client.isAuthenticated {
            configureMessagingIfPossible()
            let unresolved = conversations.compactMap { conversation -> (String, String)? in
                guard contacts.contains(where: { $0.userID == conversation.recipientUserID }) == false else {
                    return nil
                }
                return (conversation.id, conversation.recipientUserID)
            }
            for (conversationID, senderUserID) in unresolved {
                resolveIncomingUsername(
                    conversationID: conversationID,
                    senderUserID: senderUserID)
            }
        }
    }
}

extension IOSMobileAppModel: IOSDirectMessagingDelegate {
    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                     didChange state: IOSDirectMessaging.State) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.messagingState = state
            switch state {
            case .stopped:
                self.messagingStatus = "Offline"
            case .connecting:
                self.messagingStatus = "Connecting securely"
            case .ready:
                self.messagingStatus = messaging.pendingOutboxCount > 0
                    ? "Delivering queued messages" : "End-to-end encrypted"
            case .reconnecting:
                self.messagingStatus = "Reconnecting"
            case .staleCursor:
                self.messagingStatus = "Secure recovery required"
            case .authenticationRequired:
                self.messagingStatus = "Restoring session"
                self.restoreSavedSession()
            case .dependencyOutage, .failed:
                self.messagingStatus = "Messaging service unavailable"
            case .sendFailed:
                self.messagingStatus = "Delivery interrupted"
            }
        }
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                     didReceive message: IOSReceivedTextMessage) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            let received = IOSMobileMessage(
                id: UUID().uuidString.lowercased(),
                text: message.text,
                isOutgoing: false,
                sentAt: Date(timeIntervalSince1970: TimeInterval(message.sentAtMs) / 1_000),
                senderDeviceID: message.senderDeviceID)
            let knownContact = self.contacts.first(where: {
                $0.userID == message.senderUserID
            })
            let conversationID: String
            if let index = self.conversations.firstIndex(where: {
                $0.id == message.conversationID || $0.recipientUserID == message.senderUserID
            }) {
                if let knownContact {
                    self.conversations[index].handle = knownContact.handle
                    self.conversations[index].deviceCount = knownContact.deviceCount
                }
                self.conversations[index].messages.append(received)
                self.conversations[index].isSecureReady = true
                conversationID = self.conversations[index].id
            } else {
                let handle = knownContact?.handle
                    ?? "contact-\(String(message.senderUserID.prefix(8)))"
                self.conversations.insert(IOSMobileConversation(
                    id: message.conversationID,
                    handle: handle,
                    recipientUserID: message.senderUserID,
                    deviceCount: knownContact?.deviceCount ?? 1,
                    createdAt: received.sentAt,
                    messages: [received],
                    isSecureReady: true), at: 0)
                conversationID = message.conversationID
            }
            self.persistLocalState()
            if knownContact == nil {
                self.resolveIncomingUsername(
                    conversationID: conversationID,
                    senderUserID: message.senderUserID)
            }
        }
    }

    private func resolveIncomingUsername(conversationID: String, senderUserID: String) {
        guard !resolvingIncomingUserIDs.contains(senderUserID),
              let usernameAuthClient,
              let client,
              let accessToken = try? client.accessToken() else { return }
        resolvingIncomingUserIDs.insert(senderUserID)
        Task { @MainActor [weak self] in
            guard let self else { return }
            defer { self.resolvingIncomingUserIDs.remove(senderUserID) }
            do {
                let directory = try await usernameAuthClient.lookup(
                    userID: senderUserID, accessToken: accessToken)
                guard let index = self.conversations.firstIndex(where: {
                    $0.id == conversationID && $0.recipientUserID == senderUserID
                }) else { return }
                self.conversations[index].handle = directory.handle
                self.conversations[index].deviceCount = directory.devices.count
                let contact = IOSMobileContact(
                    handle: directory.handle,
                    userID: directory.userID,
                    deviceCount: directory.devices.count)
                if let contactIndex = self.contacts.firstIndex(where: {
                    $0.userID == senderUserID
                }) {
                    self.contacts[contactIndex] = contact
                } else {
                    self.contacts.append(contact)
                    self.contacts.sort { $0.handle < $1.handle }
                }
                self.persistLocalState()
            } catch {
                // Keep the non-identifying fallback; message receipt still succeeds.
            }
        }
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging) {
        directMessagingDidFail(messaging, reason: .dependencyOutage)
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                             reason: IOSMessagingIssue) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            switch reason {
            case .staleCursor:
                self.messagingStatus = "Secure recovery required"
            case .authenticationExpired:
                self.messagingStatus = "Restoring session"
                self.restoreSavedSession()
            case .dependencyOutage:
                self.messagingStatus = "Messaging service unavailable"
            case .sendFailed:
                self.messagingStatus = messaging.pendingOutboxCount > 0
                    ? "Encrypted message queued" : "Delivery interrupted"
            }
        }
    }
}
