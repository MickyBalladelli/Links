import Combine
import Foundation
import SwiftUI
import LinksClient

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

    private let client: IOSClient?
    private var messaging: IOSDirectMessaging?
    private let identityQueue = DispatchQueue(
        label: "ai.links.macos.identity", qos: .userInitiated)

    init() {
        do {
            let loadedClient = try IOSClient()
            client = loadedClient
            refreshClientState()
        } catch {
            client = nil
            identityStatus = "Identity unavailable"
            accountStatus = "Unavailable"
            deviceStatus = "Unavailable"
            onboardingError = "Saved identity metadata could not be restored."
        }
    }

    var packageStatus: String { "LinksClient + LinksKeyStore" }

    var requiresOnboarding: Bool { client?.isEnrolled != true }

    var selectedConversation: LinksMacOSConversation? {
        guard let selectedConversationID else { return nil }
        return conversations.first { $0.id == selectedConversationID }
    }

    var hasMessagingHost: Bool { messaging != nil }

    func scenePhaseDidChange(_ phase: ScenePhase) {
        switch phase {
        case .active:
            lifecycleStatus = "Active"
        case .inactive:
            lifecycleStatus = "Inactive"
        case .background:
            lifecycleStatus = "Background"
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

    /// Install the concrete shared-core host when Rust core and durable providers exist.
    /// The shell stays fail-closed until that host is provided.
    func installMessaging(factory: any SharedClientCoreFactory, endpoint: URL) throws {
        guard let client else { throw IOSClientError.identityNotEnrolled }
        messaging = IOSDirectMessaging(
            client: client,
            factory: factory,
            endpoint: endpoint,
            delegate: self)
        connectionStatus = "Offline"
        actionError = nil
    }

    func connect() {
        guard let messaging else {
            connectionStatus = "Core not configured"
            actionError = "Messaging host is not configured yet."
            return
        }
        do {
            try messaging.start()
            connectionStatus = "Connecting"
            actionError = nil
        } catch {
            connectionStatus = "Failed"
            actionError = "Connection could not start."
        }
    }

    func disconnect() {
        messaging?.stop()
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
}
