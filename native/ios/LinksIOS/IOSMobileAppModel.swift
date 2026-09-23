import Foundation
import Security
import UIKit
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
    /// Shown above incoming group messages.
    var senderUserID: String? = nil
}

/// One row of the open group's member list.
struct IOSMobileGroupMember: Identifiable, Equatable {
    let userID: String
    let handle: String?
    let role: IOSUsernameAuthClient.GroupRole?
    let isSelf: Bool

    var id: String { userID }
    var displayName: String { handle.map { "@\($0)" } ?? "Member \(userID.prefix(8))" }
}

struct IOSMobileConversation: Identifiable, Equatable, Codable {
    let id: String
    var handle: String
    let recipientUserID: String
    var deviceCount: Int
    let createdAt: Date
    var messages: [IOSMobileMessage]
    var isSecureReady: Bool
    var unreadCount: Int
    /// MLS conversation the peer last wrote in. Both sides may have created
    /// their own direct chat; replying in the peer's one uses a group both
    /// devices are members of.
    var deliveryConversationID: String?
    /// Groups keep their name in `handle`; `recipientUserID` is empty.
    var isGroup = false
    /// False once this device left or was removed; history stays readable.
    var groupActive = true

    var mlsConversationID: String { deliveryConversationID ?? id }
    var displayTitle: String { isGroup ? handle : "@\(handle)" }

    init(id: String, handle: String, recipientUserID: String, deviceCount: Int,
         createdAt: Date, messages: [IOSMobileMessage] = [], isSecureReady: Bool = false,
         unreadCount: Int = 0, deliveryConversationID: String? = nil, isGroup: Bool = false) {
        self.id = id
        self.handle = handle
        self.recipientUserID = recipientUserID
        self.deviceCount = deviceCount
        self.createdAt = createdAt
        self.messages = messages
        self.isSecureReady = isSecureReady
        self.unreadCount = unreadCount
        self.deliveryConversationID = deliveryConversationID
        self.isGroup = isGroup
    }

    private enum CodingKeys: String, CodingKey {
        case id, handle, recipientUserID, deviceCount, createdAt, messages, isSecureReady,
             unreadCount, deliveryConversationID, isGroup, groupActive
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
        unreadCount = try values.decodeIfPresent(Int.self, forKey: .unreadCount) ?? 0
        deliveryConversationID = try values.decodeIfPresent(
            String.self, forKey: .deliveryConversationID)
        isGroup = try values.decodeIfPresent(Bool.self, forKey: .isGroup) ?? false
        groupActive = try values.decodeIfPresent(Bool.self, forKey: .groupActive) ?? true
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
    @Published private(set) var profilePictureJPEG: Data?
    @Published private(set) var contactPictures: [String: Data] = [:]
    @Published private(set) var preparingConversationIDs = Set<String>()
    @Published private(set) var groupMembers: [String: [IOSMobileGroupMember]] = [:]
    @Published private(set) var groupStatus = ""
    @Published private(set) var isUpdatingGroup = false
    /// Handles learned for group members who are not saved contacts.
    fileprivate var memberHandles: [String: String] = [:]
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
    private var activeConversationID: String?
    private var isRefreshingContactPictures = false
    private var contactPictureRefreshTask: Task<Void, Never>?

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
        loadProfilePicture()
        loadCachedContactPictures()
        startContactPictureRefresh()
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
                    isSecureReady: existing.isSecureReady,
                    unreadCount: existing.unreadCount,
                    deliveryConversationID: existing.deliveryConversationID)
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
        if conversations[index].isGroup {
            await refreshGroupMembers(conversationID)
            return
        }
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
                conversationID: conversation.mlsConversationID,
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
        if conversations[index].isGroup {
            return sendGroupMessage(cleanText, conversationID: conversationID)
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
                conversationID: conversations[index].mlsConversationID,
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

    func replaceProfilePicture(with data: Data) {
        guard data.count <= Self.maximumProfilePictureBytes,
              let jpeg = Self.normalizedProfilePictureJPEG(data),
              let url = profilePictureURL else {
            error = "Use a JPEG, PNG, or HEIC picture under 8 MB."
            return
        }
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(),
                withIntermediateDirectories: true)
            try jpeg.write(to: url, options: .atomic)
            profilePictureJPEG = jpeg
            error = nil
            Task { await self.publishProfilePicture() }
        } catch {
            self.error = "The profile picture could not be saved."
        }
    }

    func removeProfilePicture() {
        guard let url = profilePictureURL else { return }
        if FileManager.default.fileExists(atPath: url.path) {
            try? FileManager.default.removeItem(at: url)
        }
        profilePictureJPEG = nil
        Task { await self.deletePublishedProfilePicture() }
    }

    func refreshContactPictures() async {
        guard let usernameAuthClient, !isRefreshingContactPictures else { return }
        isRefreshingContactPictures = true
        defer { isRefreshingContactPictures = false }
        var updated = contactPictures
        for contact in contacts {
            do {
                if let jpeg = try await usernameAuthClient.downloadProfilePicture(handle: contact.handle) {
                    updated[contact.userID] = jpeg
                    cacheContactPicture(jpeg, userID: contact.userID)
                } else if updated.removeValue(forKey: contact.userID) != nil {
                    removeCachedContactPicture(userID: contact.userID)
                }
            } catch {
                continue
            }
        }
        contactPictures = updated
    }

    private var profilePictureURL: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("profile-picture.jpg")
    }

    private func loadProfilePicture() {
        guard let url = profilePictureURL,
              let data = try? Data(contentsOf: url),
              data.count <= Self.maximumProfilePictureBytes,
              UIImage(data: data) != nil else {
            profilePictureJPEG = nil
            return
        }
        profilePictureJPEG = data
    }

    private func publishProfilePicture() async {
        guard let usernameAuthClient, let jpeg = profilePictureJPEG,
              let token = try? client?.accessToken() else { return }
        try? await usernameAuthClient.uploadProfilePicture(accessToken: token, jpeg: jpeg)
    }

    private func deletePublishedProfilePicture() async {
        guard let usernameAuthClient, let token = try? client?.accessToken() else { return }
        try? await usernameAuthClient.deleteProfilePicture(accessToken: token)
    }

    private func startContactPictureRefresh() {
        contactPictureRefreshTask?.cancel()
        contactPictureRefreshTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                await self?.publishProfilePicture()
                await self?.refreshContactPictures()
                try? await Task.sleep(nanoseconds: 10_000_000_000)
            }
        }
    }

    private var contactPictureDirectory: URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("contact-pictures", isDirectory: true)
    }

    private func loadCachedContactPictures() {
        guard let directory = contactPictureDirectory,
              let files = try? FileManager.default.contentsOfDirectory(
                at: directory, includingPropertiesForKeys: nil) else { return }
        var loaded: [String: Data] = [:]
        for file in files where file.pathExtension == "jpg" {
            let userID = file.deletingPathExtension().lastPathComponent
            guard IOSClient.isCanonicalUUID(userID),
                  let data = try? Data(contentsOf: file),
                  data.starts(with: Data([0xFF, 0xD8, 0xFF])) else { continue }
            loaded[userID] = data
        }
        contactPictures = loaded
    }

    private func cacheContactPicture(_ jpeg: Data, userID: String) {
        guard let directory = contactPictureDirectory else { return }
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try? jpeg.write(to: directory.appendingPathComponent("\(userID).jpg"), options: .atomic)
    }

    private func removeCachedContactPicture(userID: String) {
        guard let directory = contactPictureDirectory else { return }
        try? FileManager.default.removeItem(at: directory.appendingPathComponent("\(userID).jpg"))
    }

    private static let maximumProfilePictureBytes = 8 * 1024 * 1024
    private static let profilePictureSide: CGFloat = 512

    private static func normalizedProfilePictureJPEG(_ data: Data) -> Data? {
        guard let image = UIImage(data: data),
              image.size.width > 0, image.size.height > 0 else {
            return nil
        }
        let longest = max(image.size.width, image.size.height)
        let scale = min(1, profilePictureSide / longest)
        let size = CGSize(
            width: max(1, (image.size.width * scale).rounded()),
            height: max(1, (image.size.height * scale).rounded()))
        let scaled = UIGraphicsImageRenderer(size: size).image { _ in
            image.draw(in: CGRect(origin: .zero, size: size))
        }
        return scaled.jpegData(compressionQuality: 0.82)
    }

    func removeContact(_ contact: IOSMobileContact) {
        contacts.removeAll { $0.userID == contact.userID }
        persistLocalState()
    }

    func deleteConversation(_ conversation: IOSMobileConversation) {
        if conversation.isGroup, conversation.groupActive {
            Task { await leaveGroup(conversation.id) }
        }
        conversations.removeAll { $0.id == conversation.id }
        if activeConversationID == conversation.id {
            activeConversationID = nil
        }
        persistLocalState()
    }

    var unreadConversationCount: Int {
        conversations.reduce(0) { $0 + $1.unreadCount }
    }

    func markConversationRead(_ conversationID: String) {
        activeConversationID = conversationID
        guard let index = conversations.firstIndex(where: { $0.id == conversationID }),
              conversations[index].unreadCount > 0 else { return }
        conversations[index].unreadCount = 0
        persistLocalState()
    }

    func markConversationClosed(_ conversationID: String) {
        if activeConversationID == conversationID {
            activeConversationID = nil
        }
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
            if let groupIndex = self.conversations.firstIndex(where: {
                $0.isGroup && $0.id == message.conversationID
            }) {
                var groupMessage = received
                groupMessage.senderUserID = message.senderUserID
                self.conversations[groupIndex].messages.append(groupMessage)
                if self.activeConversationID != message.conversationID {
                    self.conversations[groupIndex].unreadCount += 1
                }
                self.persistLocalState()
                self.resolveMemberHandles([message.senderUserID])
                return
            }
            let knownContact = self.contacts.first(where: {
                $0.userID == message.senderUserID
            })
            let conversationID: String
            if let index = self.conversations.firstIndex(where: {
                !$0.isGroup
                    && ($0.id == message.conversationID || $0.recipientUserID == message.senderUserID)
            }) {
                if let knownContact {
                    self.conversations[index].handle = knownContact.handle
                    self.conversations[index].deviceCount = knownContact.deviceCount
                }
                self.conversations[index].messages.append(received)
                self.conversations[index].isSecureReady = true
                if self.conversations[index].mlsConversationID != message.conversationID {
                    self.conversations[index].deliveryConversationID = message.conversationID
                }
                conversationID = self.conversations[index].id
                if self.activeConversationID != conversationID {
                    self.conversations[index].unreadCount += 1
                }
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
                    isSecureReady: true,
                    unreadCount: 1), at: 0)
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

// MARK: - Group chats

extension IOSMobileAppModel {
    private var groupPrerequisites: (IOSDirectMessaging, any IOSDirectChatDirectory,
                                     IOSPreKeyHTTPClient, IOSUsernameAuthClient, String)? {
        guard let messaging, let directChatDirectory, let preKeyAPI,
              let usernameAuthClient, let token = try? client?.accessToken() else { return nil }
        return (messaging, directChatDirectory, preKeyAPI, usernameAuthClient, token)
    }

    func role(in conversationID: String) -> IOSUsernameAuthClient.GroupRole? {
        groupMembers[conversationID]?.first(where: \.isSelf)?.role
    }

    func canManageGroup(_ conversationID: String) -> Bool {
        guard conversation(withID: conversationID)?.groupActive == true else { return false }
        let role = role(in: conversationID)
        return role == .owner || role == .admin
    }

    func memberHandle(for userID: String) -> String? {
        contacts.first(where: { $0.userID == userID })?.handle ?? memberHandles[userID]
    }

    func senderLabel(for message: IOSMobileMessage) -> String? {
        guard !message.isOutgoing, let userID = message.senderUserID else { return nil }
        return memberHandle(for: userID).map { "@\($0)" } ?? "Member"
    }

    func clearGroupStatus() { groupStatus = "" }

    func createGroup(name: String, memberUserIDs: [String]) async -> IOSMobileConversation? {
        let cleanName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanName.isEmpty, cleanName.count <= 64 else {
            groupStatus = "Use a group name of 1 to 64 characters."
            return nil
        }
        guard !memberUserIDs.isEmpty else {
            groupStatus = "Choose at least one contact."
            return nil
        }
        guard let (messaging, directory, preKeyAPI, authClient, token) = groupPrerequisites,
              messaging.isConnected else {
            groupStatus = "Connect before creating a group."
            return nil
        }
        isUpdatingGroup = true
        defer { isUpdatingGroup = false }
        let groupID = UUID().uuidString.lowercased()
        do {
            try await authClient.createGroup(accessToken: token, groupID: groupID)
            for userID in memberUserIDs {
                try await authClient.setGroupRole(accessToken: token, groupID: groupID,
                                                  userID: userID, role: .member)
            }
            try messaging.createGroup(conversationID: groupID)
            try await messaging.inviteToGroup(conversationID: groupID, userIDs: memberUserIDs,
                                              directory: directory, preKeyAPI: preKeyAPI)
            try await messaging.setGroupName(conversationID: groupID, name: cleanName,
                                             directory: directory, preKeyAPI: preKeyAPI)
        } catch {
            groupStatus = "Could not create the group. Check that everyone has opened Links."
            return nil
        }
        let conversation = IOSMobileConversation(
            id: groupID, handle: cleanName, recipientUserID: "",
            deviceCount: memberUserIDs.count + 1, createdAt: Date(),
            isSecureReady: true, isGroup: true)
        conversations.insert(conversation, at: 0)
        groupStatus = ""
        persistLocalState()
        return conversation
    }

    func addMembers(_ userIDs: [String], to conversationID: String) async {
        guard let conversation = conversation(withID: conversationID), conversation.isGroup,
              !userIDs.isEmpty else { return }
        guard let (messaging, directory, preKeyAPI, authClient, token) = groupPrerequisites else {
            groupStatus = "Connect before changing the group."
            return
        }
        isUpdatingGroup = true
        defer { isUpdatingGroup = false }
        do {
            for userID in userIDs {
                try await authClient.setGroupRole(accessToken: token, groupID: conversationID,
                                                  userID: userID, role: .member)
            }
            try await messaging.inviteToGroup(conversationID: conversationID, userIDs: userIDs,
                                              directory: directory, preKeyAPI: preKeyAPI)
            try await messaging.setGroupName(conversationID: conversationID, name: conversation.handle,
                                             directory: directory, preKeyAPI: preKeyAPI)
            groupStatus = userIDs.count == 1 ? "Added 1 person." : "Added \(userIDs.count) people."
        } catch {
            groupStatus = Self.groupFailureMessage(error, action: "add people")
        }
        await refreshGroupMembers(conversationID)
    }

    func removeMember(_ userID: String, from conversationID: String) async {
        guard let (messaging, directory, preKeyAPI, authClient, token) = groupPrerequisites else {
            groupStatus = "Connect before changing the group."
            return
        }
        isUpdatingGroup = true
        defer { isUpdatingGroup = false }
        do {
            try await authClient.removeGroupMember(accessToken: token, groupID: conversationID,
                                                   userID: userID)
            try await messaging.removeFromGroup(conversationID: conversationID, userID: userID,
                                                directory: directory, preKeyAPI: preKeyAPI)
            groupStatus = "Removed \(memberHandle(for: userID).map { "@\($0)" } ?? "member")."
        } catch {
            groupStatus = Self.groupFailureMessage(error, action: "remove this person")
        }
        await refreshGroupMembers(conversationID)
    }

    func makeAdmin(_ userID: String, in conversationID: String) async {
        guard let (_, _, _, authClient, token) = groupPrerequisites else { return }
        do {
            try await authClient.setGroupRole(accessToken: token, groupID: conversationID,
                                              userID: userID, role: .admin)
        } catch {
            groupStatus = Self.groupFailureMessage(error, action: "change this role")
        }
        await refreshGroupMembers(conversationID)
    }

    func leaveGroup(_ conversationID: String) async {
        if let (messaging, _, _, authClient, token) = groupPrerequisites,
           let ownUserID = client?.userID {
            do {
                try await authClient.removeGroupMember(accessToken: token, groupID: conversationID,
                                                       userID: ownUserID)
            } catch {
                groupStatus = role(in: conversationID) == .owner
                    ? "Make another member the owner before leaving."
                    : Self.groupFailureMessage(error, action: "leave the group")
                return
            }
            try? messaging.leaveGroup(conversationID: conversationID)
        }
        if let index = conversations.firstIndex(where: { $0.id == conversationID }) {
            conversations[index].groupActive = false
        }
        groupMembers[conversationID] = []
        persistLocalState()
    }

    /// Merge MLS membership with server roles. Owners and admins also remove
    /// MLS leaves of people who already left on the server.
    func refreshGroupMembers(_ conversationID: String) async {
        guard conversation(withID: conversationID)?.groupActive == true,
              let (messaging, directory, preKeyAPI, authClient, token) = groupPrerequisites else {
            return
        }
        let mlsMembers = messaging.groupMembers(conversationID: conversationID)
        let roles: [String: IOSUsernameAuthClient.GroupRole]
        if let serverMembers = try? await authClient.groupMembers(accessToken: token,
                                                                  groupID: conversationID) {
            roles = Dictionary(serverMembers.map { ($0.userID, $0.role) },
                               uniquingKeysWith: { first, _ in first })
        } else {
            roles = [:]
        }
        let ownUserID = client?.userID
        let myRole = ownUserID.flatMap { roles[$0] }
        if myRole == .owner || myRole == .admin, !roles.isEmpty {
            for userID in mlsMembers where roles[userID] == nil && userID != ownUserID {
                try? await messaging.removeFromGroup(conversationID: conversationID, userID: userID,
                                                     directory: directory, preKeyAPI: preKeyAPI)
            }
        }
        let current = messaging.groupMembers(conversationID: conversationID)
        resolveMemberHandles(current)
        groupMembers[conversationID] = current.map { userID in
            IOSMobileGroupMember(userID: userID,
                                 handle: userID == ownUserID ? client?.accountHandle : memberHandle(for: userID),
                                 role: roles[userID], isSelf: userID == ownUserID)
        }
        .sorted { lhs, rhs in
            if lhs.isSelf != rhs.isSelf { return lhs.isSelf }
            return lhs.displayName < rhs.displayName
        }
    }

    func resolveMemberHandles(_ userIDs: [String]) {
        let unknown = userIDs.filter { memberHandle(for: $0) == nil && $0 != client?.userID }
        guard !unknown.isEmpty, let usernameAuthClient,
              let token = try? client?.accessToken() else { return }
        Task { @MainActor [weak self] in
            for userID in unknown {
                guard let directory = try? await usernameAuthClient.lookup(
                    userID: userID, accessToken: token) else { continue }
                self?.memberHandles[userID] = directory.handle
            }
            self?.objectWillChange.send()
        }
    }

    fileprivate func sendGroupMessage(_ text: String, conversationID: String) -> Bool {
        guard let (messaging, directory, preKeyAPI, _, _) = groupPrerequisites,
              messaging.isConnected else {
            error = "Messaging is reconnecting. Your text remains in the composer."
            return false
        }
        Task { @MainActor [weak self] in
            do {
                try await messaging.sendGroupText(conversationID: conversationID, text: text,
                                                  directory: directory, preKeyAPI: preKeyAPI)
                guard let self,
                      let index = self.conversations.firstIndex(where: { $0.id == conversationID }) else { return }
                self.conversations[index].messages.append(IOSMobileMessage(
                    id: UUID().uuidString.lowercased(), text: text, isOutgoing: true,
                    sentAt: Date(), senderDeviceID: nil))
                self.persistLocalState()
            } catch {
                self?.error = Self.groupFailureMessage(error, action: "send to the group")
            }
        }
        return true
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                     didReceive event: IOSGroupEvent) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.applyGroupEvent(event)
        }
    }

    private func applyGroupEvent(_ event: IOSGroupEvent) {
        let index = conversations.firstIndex(where: { $0.id == event.conversationID })
        switch event.kind {
        case .joined:
            if let index {
                conversations[index].isGroup = true
                conversations[index].groupActive = true
            } else {
                conversations.insert(IOSMobileConversation(
                    id: event.conversationID, handle: "New group", recipientUserID: "",
                    deviceCount: 0, createdAt: Date(), isSecureReady: true, isGroup: true), at: 0)
            }
        case .renamed(let name, _):
            guard let index else { return }
            conversations[index].handle = name
        case .membersChanged:
            break
        case .removed:
            guard let index else { return }
            conversations[index].groupActive = false
            groupMembers[event.conversationID] = []
        }
        persistLocalState()
        if event.kind != .removed, activeConversationID == event.conversationID {
            Task { await refreshGroupMembers(event.conversationID) }
        }
    }

    private static func groupFailureMessage(_ error: Error, action: String) -> String {
        if let authError = error as? IOSUsernameAuthError,
           case .serverRejected(let status) = authError {
            if status == 401 || status == 403 { return "Only the owner or an admin can \(action)." }
            if status == 404 { return "That account or group no longer exists." }
        }
        if let messagingError = error as? IOSMessagingError, case .notConnected = messagingError {
            return "Connect before you \(action)."
        }
        return "Could not \(action). Check that everyone has opened the latest Links."
    }
}
