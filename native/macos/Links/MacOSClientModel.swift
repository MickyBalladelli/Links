import Combine
import Foundation
import SwiftUI
import AppKit
import LinksClient
import LinksKeyStore

struct LinksMacOSMessage: Identifiable, Equatable, Codable {
    let id: UUID
    let text: String
    let isOutgoing: Bool
    let sentAt: Date
    let senderDeviceID: String?
}

struct LinksMacOSConversation: Identifiable, Equatable, Codable {
    let id: String
    var title: String
    let recipientUserID: String
    var messages: [LinksMacOSMessage]
}

enum LinksMacOSAuthMode: String, CaseIterable, Identifiable {
    case register
    case login

    var id: String { rawValue }
    var title: String { rawValue == "register" ? "Register" : "Log in" }
}

enum LinksMacOSDeliveryState: Equatable {
    case notConfigured
    case offline
    case connecting
    case ready
    case reconnecting
    case offlineOutboxRetry(count: Int)
    case staleCursor
    case authenticationExpired
    case sendFailed
    case dependencyOutage

    var title: String {
        switch self {
        case .notConfigured: return "Not configured"
        case .offline: return "Offline"
        case .connecting: return "Connecting"
        case .ready: return "Ready"
        case .reconnecting: return "Reconnecting"
        case .offlineOutboxRetry: return "Retrying encrypted outbox"
        case .staleCursor: return "Recovery needed"
        case .authenticationExpired: return "Sign-in required"
        case .sendFailed: return "Send failed"
        case .dependencyOutage: return "Service unavailable"
        }
    }

    var detail: String {
        switch self {
        case .notConfigured:
            return "Install the shared Rust core and host providers."
        case .offline:
            return "Messages stay local until the connection returns."
        case .connecting:
            return "Opening the encrypted connection."
        case .ready:
            return "Encrypted connection is ready."
        case .reconnecting:
            return "Connection dropped. Retrying automatically."
        case .offlineOutboxRetry(let count):
            let suffix = count == 1 ? "" : "s"
            return "\(count) encrypted message\(suffix) waiting to retry."
        case .staleCursor:
            return "Mailbox cursor expired. Full recovery is required."
        case .authenticationExpired:
            return "Sign in again to reconnect."
        case .sendFailed:
            return "The message was not accepted. Pending encrypted data is preserved."
        case .dependencyOutage:
            return "The account or messaging service is unavailable."
        }
    }

    var systemImage: String {
        switch self {
        case .ready: return "checkmark.circle.fill"
        case .connecting, .reconnecting, .offlineOutboxRetry: return "arrow.clockwise"
        case .staleCursor: return "arrow.triangle.2.circlepath"
        case .authenticationExpired: return "person.crop.circle.badge.exclamationmark"
        case .sendFailed: return "exclamationmark.bubble"
        case .dependencyOutage: return "network.slash"
        case .notConfigured, .offline: return "wifi.slash"
        }
    }
}

private final class IOSClientBox: @unchecked Sendable {
    let client: IOSClient

    init(_ client: IOSClient) {
        self.client = client
    }
}

private struct LinksMacOSPersistedState: Codable {
    let conversations: [LinksMacOSConversation]
    let selectedConversationID: String?
}

@MainActor
final class LinksMacOSAppModel: ObservableObject, IOSDirectMessagingDelegate {
    @Published private(set) var identityStatus = "Checking identity"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device enrolled"
    @Published private(set) var connectionStatus = "Core not configured"
    @Published private(set) var deliveryState = LinksMacOSDeliveryState.notConfigured
    @Published private(set) var pendingOutboxCount = 0
    @Published private(set) var lifecycleStatus = "Launching"
    @Published private(set) var conversations: [LinksMacOSConversation] = []
    @Published var selectedConversationID: String?
    @Published var composerText = ""
    @Published private(set) var onboardingError: String?
    @Published var actionError: String?
    @Published private(set) var isEnrolling = false
    @Published private(set) var profileName = ClientProfile.default.name
    @Published private(set) var profileRootPath = ""
    @Published private(set) var profileLogPath = ""
    @Published var usernameInput = ""
    @Published var authMode: LinksMacOSAuthMode = .register
    @Published private(set) var authEndpointText = "http://127.0.0.1:8080"
    @Published private(set) var isAuthenticating = false
    @Published var phoneInput = ""
    @Published var otpCodeInput = ""
    @Published var otpChannel: IOSOTPChannel = .sms
    @Published private(set) var otpStatus = "Phone OTP not started"
    @Published private(set) var isOTPWorking = false
    @Published private(set) var preKeyStatus = "Pre-key inventory not initialized"
    @Published private(set) var conversationSetupStatus = "MLS conversation not initialized"
    @Published var pairingTarget = ""
    @Published var pairingInput = ""
    @Published private(set) var pairingURI: String?
    @Published private(set) var pairingStatus = "No pairing activity"
    @Published private(set) var isPairing = false

    private let client: IOSClient?
    private let authClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private let preKeyAPI: IOSPreKeyHTTPClient?
    private var encryptedStateStore: MacOSEncryptedStateStore?
    private(set) var durableMessagingStore: MacOSDurableMessagingStore?
    private var otpChallenge: IOSOTPChallenge?
    private var messaging: IOSDirectMessaging?
    private var directChatDirectory: (any IOSDirectChatDirectory)?
    private var profileLogger: LinksMacOSProfileLogger?
    private let identityQueue = DispatchQueue(
        label: "ai.links.macos.identity", qos: .userInitiated)
    private var connectionRequested = false
    private var reconnectAfterBackground = false

    init() {
        do {
            let profile = try Self.profileFromArguments()
            let (profileRoot, hasExplicitRoot) = try Self.profileRootFromArguments(profile: profile)
            profileRootPath = profileRoot.url.path
            profileLogPath = profileRoot.logsURL
                .appendingPathComponent("client.log", isDirectory: false).path
            profileLogger = try? LinksMacOSProfileLogger(root: profileRoot)
            encryptedStateStore = try? MacOSEncryptedStateStore(
                profile: profile,
                rootURL: profileRoot.url,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            durableMessagingStore = try? MacOSDurableMessagingStore(
                profile: profile,
                rootURL: profileRoot.url,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            let provider = MacOSKeychainSeedProvider(
                profile: profile,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            let identityStore = HardwareIdentityStore(seedProvider: provider)
            let metadataDefaults = try Self.metadataDefaults(
                for: profileRoot, hasExplicitRoot: hasExplicitRoot)
            let loadedClient = try IOSClient(
                identityStore: identityStore,
                defaults: metadataDefaults,
                profile: profile)
            profileName = profile.name
            client = loadedClient
            usernameInput = loadedClient.accountHandle ?? ""
            if let endpoint = try? Self.authEndpointFromArguments() {
                let session = URLSession(configuration: .ephemeral)
                authClient = try? IOSUsernameAuthClient(baseURL: endpoint, urlSession: session)
                otpClient = try? IOSOTPClient(baseURL: endpoint, urlSession: session)
                preKeyAPI = try? IOSPreKeyHTTPClient(baseURL: endpoint, urlSession: session)
                if authClient != nil {
                    authEndpointText = endpoint.absoluteString
                } else {
                    authEndpointText = "Invalid local auth endpoint"
                }
            } else {
                authClient = nil
                otpClient = nil
                preKeyAPI = nil
                authEndpointText = "Invalid local auth endpoint"
            }
            restoreLocalState()
            refreshClientState()
            profileLogger?.record(.launched)
        } catch {
            client = nil
            authClient = nil
            otpClient = nil
            preKeyAPI = nil
            encryptedStateStore = nil
            durableMessagingStore = nil
            profileLogger = nil
            profileRootPath = "Invalid profile root"
            profileLogPath = "Unavailable"
            profileName = "Invalid profile"
            identityStatus = "Identity unavailable"
            accountStatus = "Unavailable"
            deviceStatus = "Unavailable"
            onboardingError = "Saved identity metadata could not be restored."
        }
    }

    var packageStatus: String { "LinksClient + LinksKeyStore" }

    var requiresOnboarding: Bool { client?.isEnrolled != true }

    var requiresAccountAuthentication: Bool {
        client?.isEnrolled == true && client?.isAuthenticated != true
    }

    var otpAvailable: Bool { otpClient != nil }

    var hasOTPChallenge: Bool { otpChallenge != nil }

    var selectedConversation: LinksMacOSConversation? {
        guard let selectedConversationID else { return nil }
        return conversations.first { $0.id == selectedConversationID }
    }

    var hasMessagingHost: Bool { messaging != nil }

    var isConnectionRequested: Bool { connectionRequested }

    func scenePhaseDidChange(_ phase: ScenePhase) {
        switch phase {
        case .active:
            profileLogger?.record(.active)
            lifecycleStatus = "Active"
            if reconnectAfterBackground {
                reconnectAfterBackground = false
                connect()
            }
        case .inactive:
            profileLogger?.record(.inactive)
            lifecycleStatus = "Inactive"
        case .background:
            profileLogger?.record(.background)
            lifecycleStatus = "Background"
            reconnectAfterBackground = connectionRequested && messaging != nil
            messaging?.shutdown()
            connectionStatus = "Offline"
            deliveryState = .offline
        @unknown default:
            lifecycleStatus = "Unknown"
        }
    }

    func enrollIdentity() {
        guard let client, !client.isEnrolled, !isEnrolling else { return }
        isEnrolling = true
        onboardingError = nil
        let clientBox = IOSClientBox(client)
        identityQueue.async { [weak self, clientBox] in
            do {
                _ = try clientBox.client.createIdentity()
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.isEnrolling = false
                    self.refreshClientState()
                }
            } catch {
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.isEnrolling = false
                    self.onboardingError = "Secure identity creation failed on this Mac."
                }
            }
        }
    }

    func authenticateUsername() {
        guard let client, let authClient, client.isEnrolled, !isAuthenticating else {
            onboardingError = "Local username auth is unavailable."
            return
        }
        let handle = usernameInput
        let mode = authMode
        isAuthenticating = true
        onboardingError = nil
        Task { @MainActor [weak self] in
            do {
                switch mode {
                case .register:
                    _ = try await client.registerUsername(using: authClient, handle: handle)
                case .login:
                    _ = try await client.loginUsername(using: authClient, handle: handle)
                }
                guard let self else { return }
                self.isAuthenticating = false
                self.usernameInput = client.accountHandle ?? handle
                self.refreshClientState()
            } catch {
                guard let self else { return }
                self.isAuthenticating = false
                self.onboardingError = "Username auth failed. Check the handle and local auth service."
            }
        }
    }

    func startOTPEnrollment() {
        guard let client, let otpClient, client.isEnrolled, !isOTPWorking else {
            otpStatus = "Use an HTTPS account-auth endpoint for phone OTP"
            return
        }
        let phone = phoneInput.trimmingCharacters(in: .whitespacesAndNewlines)
        isOTPWorking = true
        otpStatus = "Sending verification code"
        Task { @MainActor [weak self] in
            do {
                let challenge = try await client.startOTP(
                    using: otpClient, phone: phone, channel: self?.otpChannel ?? .sms)
                guard let self else { return }
                self.otpChallenge = challenge
                self.otpCodeInput = ""
                self.isOTPWorking = false
                self.otpStatus = "Code sent. It expires in 10 minutes."
            } catch {
                guard let self else { return }
                self.isOTPWorking = false
                self.otpStatus = "Could not send verification code"
            }
        }
    }

    func finishOTPEnrollment() {
        guard let client, let otpClient, let challenge = otpChallenge, !isOTPWorking else {
            otpStatus = "Request a verification code first"
            return
        }
        let code = otpCodeInput.trimmingCharacters(in: .whitespacesAndNewlines)
        otpCodeInput = ""
        isOTPWorking = true
        otpStatus = "Checking verification code"
        Task { @MainActor [weak self] in
            do {
                _ = try await client.finishOTP(using: otpClient, challenge: challenge, code: code)
                guard let self else { return }
                self.otpChallenge = nil
                self.isOTPWorking = false
                self.otpStatus = "Phone account enrolled"
                self.refreshClientState()
            } catch {
                guard let self else { return }
                self.isOTPWorking = false
                self.otpStatus = "Verification failed. Request a new code if it expired."
            }
        }
    }

    func createPairingLink() {
        guard let client, client.isEnrolled, !isPairing else { return }
        guard let authClient else {
            pairingStatus = "Local auth service is unavailable"
            return
        }
        let target = pairingTarget.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !target.isEmpty else {
            pairingStatus = "Enter an account username or user ID"
            return
        }
        isPairing = true
        pairingStatus = "Finding account"
        Task { @MainActor [weak self] in
            do {
                let userID: String
                let cleanTarget = target.lowercased()
                if IOSClient.isCanonicalUUID(cleanTarget) {
                    userID = cleanTarget
                } else {
                    userID = try await authClient.lookup(handle: cleanTarget).userID
                }
                let uri = try client.makePairingURI(for: userID)
                guard let self else { return }
                self.pairingURI = uri
                self.isPairing = false
                self.pairingStatus = "Pairing link ready. Scan it on the authenticated device."
            } catch {
                guard let self else { return }
                self.isPairing = false
                self.pairingStatus = "Could not create pairing link"
            }
        }
    }

    func copyPairingLink() {
        guard let pairingURI else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(pairingURI, forType: .string)
        pairingStatus = "Pairing link copied"
    }

    func approvePairing() {
        guard let client, let authClient, client.isAuthenticated, !isPairing else { return }
        let uri = pairingInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !uri.isEmpty else {
            pairingStatus = "Paste a links://connect pairing link"
            return
        }
        isPairing = true
        pairingStatus = "Checking pairing link"
        Task { @MainActor [weak self] in
            do {
                let response = try await client.approvePairing(using: authClient, uri: uri)
                guard let self else { return }
                self.isPairing = false
                self.pairingInput = ""
                self.pairingStatus = "Device approved: \(self.shortID(response.deviceID))"
            } catch {
                guard let self else { return }
                self.isPairing = false
                self.pairingStatus = "Pairing approval failed"
            }
        }
    }

    func handleIncomingURL(_ url: URL) {
        guard url.scheme?.lowercased() == "links",
              url.host?.lowercased() == "connect",
              url.path.isEmpty || url.path == "/",
              url.query?.isEmpty == false else {
            actionError = "Invalid Links pairing link."
            return
        }
        pairingInput = url.absoluteString
        pairingStatus = "Pairing link received. Review before approval."
    }

    /// Install the concrete shared-core host when Rust core and durable providers exist.
    /// The shell stays fail-closed until that host is provided.
    func installMessaging(factory: any SharedClientCoreFactory, endpoint: URL) throws {
        try installMessaging(factory: factory, endpoint: endpoint, directory: nil)
    }

    /// Install the shared-core host and its authenticated recipient directory.
    /// The directory must return a current MLS KeyPackage for every active
    /// recipient device; no placeholder or UI-owned crypto is accepted.
    func installMessaging(factory: any SharedClientCoreFactory, endpoint: URL,
                          directory: (any IOSDirectChatDirectory)?) throws {
        guard let client else { throw IOSClientError.identityNotEnrolled }
        messaging = IOSDirectMessaging(
            client: client,
            factory: factory,
            endpoint: endpoint,
            delegate: self)
        directChatDirectory = directory
        connectionRequested = false
        reconnectAfterBackground = false
        connectionStatus = "Offline"
        deliveryState = .offline
        pendingOutboxCount = 0
        actionError = nil
    }

    func connect() {
        guard let messaging else {
            connectionStatus = "Core not configured"
            deliveryState = .notConfigured
            actionError = "Messaging host is not configured yet."
            return
        }
        connectionRequested = true
        deliveryState = .connecting
        connectionStatus = deliveryState.title
        do {
            try messaging.start()
            profileLogger?.record(.connectionStarted)
            pendingOutboxCount = messaging.pendingOutboxCount
            actionError = nil
            if let preKeyAPI {
                preKeyStatus = "Preparing pre-key inventory"
                Task { @MainActor [weak self] in
                    guard let self, let messaging = self.messaging else { return }
                    do {
                        let inventory = try await messaging.maintainPreKeyInventory(
                            using: preKeyAPI)
                        self.preKeyStatus = "Ready: \(inventory.oneTimeCurvePreKeys) curve, "
                            + "\(inventory.oneTimeKEMPreKeys) KEM keys"
                    } catch {
                        self.preKeyStatus = "Pre-key setup failed"
                        self.actionError = "Initial pre-key inventory could not be uploaded."
                    }
                }
            }
        } catch {
            profileLogger?.record(.connectionFailed)
            connectionRequested = false
            deliveryState = .dependencyOutage
            connectionStatus = deliveryState.title
            actionError = "Connection could not start."
        }
    }

    func disconnect() {
        connectionRequested = false
        reconnectAfterBackground = false
        messaging?.stop()
        profileLogger?.record(.connectionStopped)
        deliveryState = .offline
        connectionStatus = deliveryState.title
    }

    func recoverStaleCursor() {
        guard let messaging else {
            deliveryState = .notConfigured
            connectionStatus = deliveryState.title
            return
        }
        guard deliveryState == .staleCursor else { return }
        deliveryState = .connecting
        connectionStatus = deliveryState.title
        do {
            try messaging.recoverFromStaleCursor()
            actionError = nil
        } catch {
            deliveryState = .staleCursor
            connectionStatus = deliveryState.title
            actionError = "Full mailbox recovery is unavailable on this host."
        }
    }

    /// Stop transport before process termination. The shared core owns the
    /// durable outbox and cursor, so shutdown releases transport state without
    /// replacing or clearing pending encrypted work.
    func shutdownForTermination() {
        connectionRequested = false
        reconnectAfterBackground = false
        lifecycleStatus = "Stopping"
        profileLogger?.record(.stopping)
        messaging?.shutdown()
        deliveryState = .offline
        connectionStatus = deliveryState.title
    }

    func createConversation(title: String, recipientUserID: String) -> Bool {
        let cleanTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanRecipient = recipientUserID
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard !cleanTitle.isEmpty,
              IOSClient.isCanonicalUUID(cleanRecipient) else {
            actionError = "Use a name and a valid recipient user ID."
            return false
        }
        let conversation = LinksMacOSConversation(
            id: UUID().uuidString.lowercased(),
            title: cleanTitle,
            recipientUserID: cleanRecipient,
            messages: [])
        conversations.append(conversation)
        selectedConversationID = conversation.id
        actionError = nil
        persistLocalState()
        return true
    }

    /// Claim and verify recipient pre-keys, then stage the first two-user MLS
    /// conversation in the shared client core.
    func initializeSelectedConversation() {
        guard let selectedConversationID,
              let conversation = conversations.first(where: {
                  $0.id == selectedConversationID
              }),
              let messaging,
              let preKeyAPI,
              let directChatDirectory else {
            conversationSetupStatus = "MLS host directory is not configured"
            actionError = "Configure an authenticated directory with MLS KeyPackages first."
            return
        }
        conversationSetupStatus = "Claiming recipient pre-keys"
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await messaging.initializeFirstDirectConversation(
                    conversationID: conversation.id,
                    recipientUserID: conversation.recipientUserID,
                    directory: directChatDirectory,
                    preKeyAPI: preKeyAPI)
                self.conversationSetupStatus = "Secure two-user MLS conversation ready"
                self.actionError = nil
            } catch {
                self.conversationSetupStatus = "MLS conversation setup failed"
                self.actionError = "Recipient pre-key verification or MLS setup failed."
            }
        }
    }

    func sendMessage() {
        let text = composerText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty,
              text.utf8.count <= IOSDirectMessaging.maximumTextBytes,
              let selectedConversationID,
              let index = conversations.firstIndex(where: { $0.id == selectedConversationID }) else {
            actionError = "Select a conversation and enter a message under 64 KiB."
            return
        }
        let conversation = conversations[index]
        guard let messaging else {
            actionError = "Messaging host is not configured yet."
            return
        }
        do {
            try messaging.sendText(
                conversationID: conversation.id,
                recipientUserID: conversation.recipientUserID,
                text: text)
            pendingOutboxCount = messaging.pendingOutboxCount
            conversations[index].messages.append(LinksMacOSMessage(
                id: UUID(),
                text: text,
                isOutgoing: true,
                sentAt: Date(),
                senderDeviceID: nil))
            composerText = ""
            actionError = nil
            persistLocalState()
        } catch {
            pendingOutboxCount = messaging.pendingOutboxCount
            if pendingOutboxCount > 0 {
                deliveryState = .offlineOutboxRetry(count: pendingOutboxCount)
                connectionStatus = deliveryState.title
                actionError = "Message queued in the encrypted outbox for retry."
            } else if messaging.state == .connecting
                        || messaging.state == .reconnecting
                        || messaging.state == .dependencyOutage {
                deliveryState = .reconnecting
                connectionStatus = deliveryState.title
                actionError = "Offline. Message stays in the composer until reconnect."
            } else {
                deliveryState = .sendFailed
                connectionStatus = deliveryState.title
                actionError = "Message was not sent. Check the connection."
            }
        }
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                    didChange state: IOSDirectMessaging.State) {
        Task { @MainActor [weak self] in
            self?.handleMessagingStateChange(state, messaging: messaging)
        }
    }

    private func handleMessagingStateChange(_ state: IOSDirectMessaging.State,
                                            messaging: IOSDirectMessaging) {
        pendingOutboxCount = messaging.pendingOutboxCount
        switch state {
        case .stopped:
            deliveryState = .offline
        case .connecting:
            deliveryState = connectionRequested ? .connecting : .offline
        case .ready:
            deliveryState = pendingOutboxCount > 0
                ? .offlineOutboxRetry(count: pendingOutboxCount) : .ready
            if pendingOutboxCount == 0 { actionError = nil }
        case .reconnecting:
            deliveryState = pendingOutboxCount > 0
                ? .offlineOutboxRetry(count: pendingOutboxCount) : .reconnecting
        case .staleCursor:
            deliveryState = .staleCursor
        case .authenticationRequired:
            deliveryState = .authenticationExpired
            client?.clearAuthenticatedSession()
            connectionRequested = false
        case .dependencyOutage:
            deliveryState = .dependencyOutage
        case .sendFailed:
            deliveryState = .sendFailed
        case .failed:
            deliveryState = .dependencyOutage
        }
        connectionStatus = deliveryState.title
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                    didReceive message: IOSReceivedTextMessage) {
        Task { @MainActor [weak self] in
            self?.renderReceivedMessage(message)
        }
    }

    private func renderReceivedMessage(_ message: IOSReceivedTextMessage) {
        let received = LinksMacOSMessage(
            id: UUID(),
            text: message.text,
            isOutgoing: false,
            sentAt: Date(timeIntervalSince1970: TimeInterval(message.sentAtMs) / 1000),
            senderDeviceID: message.senderDeviceID)
        if let index = conversations.firstIndex(where: { $0.id == message.conversationID }) {
            conversations[index].messages.append(received)
        } else {
            conversations.append(LinksMacOSConversation(
                id: message.conversationID,
                title: "Incoming conversation",
                recipientUserID: "",
                messages: [received]))
        }
        selectedConversationID = message.conversationID
        persistLocalState()
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging) {
        Task { @MainActor [weak self] in
            self?.handleMessagingFailure(.dependencyOutage, messaging: messaging)
        }
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                            reason: IOSMessagingIssue) {
        Task { @MainActor [weak self] in
            self?.handleMessagingFailure(reason, messaging: messaging)
        }
    }

    private func handleMessagingFailure(_ reason: IOSMessagingIssue,
                                        messaging: IOSDirectMessaging) {
        pendingOutboxCount = messaging.pendingOutboxCount
        switch reason {
        case .staleCursor:
            deliveryState = .staleCursor
        case .authenticationExpired:
            deliveryState = .authenticationExpired
            client?.clearAuthenticatedSession()
            connectionRequested = false
        case .dependencyOutage:
            deliveryState = .dependencyOutage
        case .sendFailed:
            deliveryState = pendingOutboxCount > 0
                ? .offlineOutboxRetry(count: pendingOutboxCount) : .sendFailed
        }
        connectionStatus = deliveryState.title
        actionError = deliveryState.detail
    }

    private func refreshClientState() {
        guard let client else { return }
        identityStatus = client.isEnrolled ? "Identity enrolled" : "Identity not enrolled"
        accountStatus = client.isAuthenticated ? "Authenticated" : "Signed out"
        if let deviceID = client.deviceID, let mlsNodeID = client.mlsNodeID {
            deviceStatus = "Device \(shortID(deviceID)) · Node \(shortID(mlsNodeID))"
        } else {
            deviceStatus = "No device enrolled"
        }
    }

    private func restoreLocalState() {
        guard let encryptedStateStore else { return }
        do {
            guard let encoded = try encryptedStateStore.read() else { return }
            let state = try PropertyListDecoder().decode(
                LinksMacOSPersistedState.self, from: encoded)
            guard state.conversations.count <= 5_000,
                  state.conversations.allSatisfy(isValidConversation) else {
                throw MacOSEncryptedStateStore.StateError.invalidState
            }
            conversations = state.conversations
            selectedConversationID = state.selectedConversationID.flatMap { selectedID in
                state.conversations.contains(where: { $0.id == selectedID }) ? selectedID : nil
            }
        } catch {
            conversations = []
            selectedConversationID = nil
            actionError = "Encrypted local state could not be restored."
        }
    }

    private func persistLocalState() {
        guard let encryptedStateStore else { return }
        let state = LinksMacOSPersistedState(
            conversations: conversations,
            selectedConversationID: selectedConversationID)
        do {
            let encoded = try PropertyListEncoder().encode(state)
            try encryptedStateStore.write(encoded)
        } catch {
            actionError = "Encrypted local state could not be saved."
        }
    }

    private func isValidConversation(_ conversation: LinksMacOSConversation) -> Bool {
        guard IOSClient.isCanonicalUUID(conversation.id),
              !conversation.title.isEmpty,
              conversation.title.utf8.count <= 256,
              conversation.recipientUserID.isEmpty
                  || IOSClient.isCanonicalUUID(conversation.recipientUserID),
              conversation.messages.count <= 10_000 else {
            return false
        }
        return conversation.messages.allSatisfy { message in
            !message.text.isEmpty
                && message.text.utf8.count <= IOSDirectMessaging.maximumTextBytes
                && message.sentAt.timeIntervalSince1970.isFinite
                && (message.senderDeviceID == nil
                    || IOSClient.isCanonicalUUID(message.senderDeviceID!))
        }
    }

    private func shortID(_ value: String) -> String {
        String(value.prefix(8))
    }

    private static func profileFromArguments() throws -> ClientProfile {
        let arguments = CommandLine.arguments
        guard let marker = arguments.firstIndex(of: "--profile") else {
            return .default
        }
        guard arguments.index(after: marker) < arguments.endIndex else {
            throw ClientProfileError.invalidName
        }
        return try ClientProfile(name: arguments[arguments.index(after: marker)])
    }

    private static func profileRootFromArguments(profile: ClientProfile)
        throws -> (MacOSProfileRoot, Bool) {
        let arguments = CommandLine.arguments
        let argumentRoot: String?
        if let marker = arguments.firstIndex(of: "--profile-root") {
            guard arguments.index(after: marker) < arguments.endIndex else {
                throw MacOSProfileRoot.RootError.invalidPath
            }
            argumentRoot = arguments[arguments.index(after: marker)]
        } else {
            argumentRoot = nil
        }
        let rawRoot = argumentRoot ?? ProcessInfo.processInfo.environment["LINKS_PROFILE_ROOT"]
        guard let rawRoot else {
            return (try MacOSProfileRoot(profile: profile), false)
        }
        let expandedRoot = (rawRoot as NSString).expandingTildeInPath
        guard !expandedRoot.isEmpty, expandedRoot.hasPrefix("/") else {
            throw MacOSProfileRoot.RootError.invalidPath
        }
        let baseURL = URL(fileURLWithPath: expandedRoot, isDirectory: true)
        return (try MacOSProfileRoot(profile: profile, baseURL: baseURL), true)
    }

    private static func metadataDefaults(for profileRoot: MacOSProfileRoot,
                                         hasExplicitRoot: Bool) throws -> UserDefaults {
        if !hasExplicitRoot && profileRoot.profile == .default {
            return .standard
        }
        guard let defaults = UserDefaults(suiteName: profileRoot.metadataSuiteName) else {
            throw MacOSProfileRoot.RootError.metadataUnavailable
        }
        return defaults
    }

    private static func authEndpointFromArguments() throws -> URL {
        let arguments = CommandLine.arguments
        let raw: String
        if let marker = arguments.firstIndex(of: "--auth-url"),
           arguments.index(after: marker) < arguments.endIndex {
            raw = arguments[arguments.index(after: marker)]
        } else {
            raw = ProcessInfo.processInfo.environment["LINKS_AUTH_URL"]
                ?? "http://127.0.0.1:8080"
        }
        guard let endpoint = URL(string: raw) else {
            throw IOSUsernameAuthError.invalidEndpoint
        }
        return endpoint
    }
}
