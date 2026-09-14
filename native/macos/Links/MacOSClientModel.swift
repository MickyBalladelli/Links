import Combine
import Foundation
import SwiftUI
import AppKit
import LinksClient
import LinksKeyStore

struct LinksMacOSMessage: Identifiable, Equatable {
    let id: UUID
    let text: String
    let isOutgoing: Bool
    let sentAt: Date
    let senderDeviceID: String?
}

struct LinksMacOSConversation: Identifiable, Equatable {
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

private final class IOSClientBox: @unchecked Sendable {
    let client: IOSClient

    init(_ client: IOSClient) {
        self.client = client
    }
}

@MainActor
final class LinksMacOSAppModel: ObservableObject, IOSDirectMessagingDelegate {
    @Published private(set) var identityStatus = "Checking identity"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device enrolled"
    @Published private(set) var connectionStatus = "Core not configured"
    @Published private(set) var lifecycleStatus = "Launching"
    @Published private(set) var conversations: [LinksMacOSConversation] = []
    @Published var selectedConversationID: String?
    @Published var composerText = ""
    @Published private(set) var onboardingError: String?
    @Published var actionError: String?
    @Published private(set) var isEnrolling = false
    @Published private(set) var profileName = ClientProfile.default.name
    @Published var usernameInput = ""
    @Published var authMode: LinksMacOSAuthMode = .register
    @Published private(set) var authEndpointText = "http://127.0.0.1:8080"
    @Published private(set) var isAuthenticating = false
    @Published var phoneInput = ""
    @Published var otpCodeInput = ""
    @Published var otpChannel: IOSOTPChannel = .sms
    @Published private(set) var otpStatus = "Phone OTP not started"
    @Published private(set) var isOTPWorking = false
    @Published var pairingTarget = ""
    @Published var pairingInput = ""
    @Published private(set) var pairingURI: String?
    @Published private(set) var pairingStatus = "No pairing activity"
    @Published private(set) var isPairing = false

    private let client: IOSClient?
    private let authClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private var otpChallenge: IOSOTPChallenge?
    private var messaging: IOSDirectMessaging?
    private let identityQueue = DispatchQueue(
        label: "ai.links.macos.identity", qos: .userInitiated)
    private var connectionRequested = false
    private var reconnectAfterBackground = false

    init() {
        do {
            let profile = try Self.profileFromArguments()
            let provider = MacOSKeychainSeedProvider(profile: profile)
            let identityStore = HardwareIdentityStore(seedProvider: provider)
            let loadedClient = try IOSClient(
                identityStore: identityStore,
                profile: profile)
            profileName = profile.name
            client = loadedClient
            usernameInput = loadedClient.accountHandle ?? ""
            if let endpoint = try? Self.authEndpointFromArguments(),
               let loadedAuthClient = try? IOSUsernameAuthClient(baseURL: endpoint) {
                authClient = loadedAuthClient
                otpClient = try? IOSOTPClient(baseURL: endpoint)
                authEndpointText = endpoint.absoluteString
            } else {
                authClient = nil
                otpClient = nil
                authEndpointText = "Invalid local auth endpoint"
            }
            refreshClientState()
        } catch {
            client = nil
            authClient = nil
            otpClient = nil
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

    func scenePhaseDidChange(_ phase: ScenePhase) {
        switch phase {
        case .active:
            lifecycleStatus = "Active"
            if reconnectAfterBackground {
                reconnectAfterBackground = false
                connect()
            }
        case .inactive:
            lifecycleStatus = "Inactive"
        case .background:
            lifecycleStatus = "Background"
            reconnectAfterBackground = connectionRequested && messaging != nil
            messaging?.shutdown()
            connectionStatus = "Offline"
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
        guard let client else { throw IOSClientError.identityNotEnrolled }
        messaging = IOSDirectMessaging(
            client: client,
            factory: factory,
            endpoint: endpoint,
            delegate: self)
        connectionRequested = false
        reconnectAfterBackground = false
        connectionStatus = "Offline"
        actionError = nil
    }

    func connect() {
        guard let messaging else {
            connectionStatus = "Core not configured"
            actionError = "Messaging host is not configured yet."
            return
        }
        connectionRequested = true
        do {
            try messaging.start()
            connectionStatus = "Connecting"
            actionError = nil
        } catch {
            connectionRequested = false
            connectionStatus = "Failed"
            actionError = "Connection could not start."
        }
    }

    func disconnect() {
        connectionRequested = false
        reconnectAfterBackground = false
        messaging?.stop()
        connectionStatus = "Offline"
    }

    /// Stop transport before process termination. The shared core owns the
    /// durable outbox and cursor, so shutdown releases transport state without
    /// replacing or clearing pending encrypted work.
    func shutdownForTermination() {
        connectionRequested = false
        reconnectAfterBackground = false
        lifecycleStatus = "Stopping"
        messaging?.shutdown()
        connectionStatus = "Offline"
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
        return true
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
            conversations[index].messages.append(LinksMacOSMessage(
                id: UUID(),
                text: text,
                isOutgoing: true,
                sentAt: Date(),
                senderDeviceID: nil))
            composerText = ""
            actionError = nil
        } catch {
            actionError = "Message was not sent. Check the connection."
        }
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                    didChange state: IOSDirectMessaging.State) {
        Task { @MainActor [weak self] in
            self?.handleMessagingStateChange(state)
        }
    }

    private func handleMessagingStateChange(_ state: IOSDirectMessaging.State) {
        switch state {
        case .stopped:
            connectionStatus = "Offline"
        case .connecting:
            connectionStatus = "Connecting"
        case .ready:
            connectionStatus = "Ready"
            actionError = nil
        case .failed:
            connectionStatus = "Failed"
        }
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
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging) {
        Task { @MainActor [weak self] in
            self?.handleMessagingFailure()
        }
    }

    private func handleMessagingFailure() {
        connectionStatus = "Failed"
        actionError = "Encrypted messaging core failed."
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
