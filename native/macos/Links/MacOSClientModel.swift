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
    var recipientUserID: String
    var peerUserID: String?
    var messages: [LinksMacOSMessage]
    var unreadCount: Int

    var isIncoming: Bool { recipientUserID.isEmpty }

    init(id: String, title: String, recipientUserID: String,
         peerUserID: String? = nil, messages: [LinksMacOSMessage], unreadCount: Int = 0) {
        self.id = id
        self.title = title
        self.recipientUserID = recipientUserID
        self.peerUserID = peerUserID
        self.messages = messages
        self.unreadCount = unreadCount
    }

    private enum CodingKeys: String, CodingKey {
        case id, title, recipientUserID, peerUserID, messages, unreadCount
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        id = try values.decode(String.self, forKey: .id)
        title = try values.decode(String.self, forKey: .title)
        recipientUserID = try values.decode(String.self, forKey: .recipientUserID)
        peerUserID = try values.decodeIfPresent(String.self, forKey: .peerUserID)
        messages = try values.decodeIfPresent([LinksMacOSMessage].self, forKey: .messages) ?? []
        unreadCount = try values.decodeIfPresent(Int.self, forKey: .unreadCount) ?? 0
    }
}

struct LinksMacOSContact: Identifiable, Equatable, Codable {
    let handle: String
    let userID: String
    let deviceCount: Int

    var id: String { userID }
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
    let contacts: [LinksMacOSContact]

    init(conversations: [LinksMacOSConversation], selectedConversationID: String?,
         contacts: [LinksMacOSContact]) {
        self.conversations = conversations
        self.selectedConversationID = selectedConversationID
        self.contacts = contacts
    }

    private enum CodingKeys: String, CodingKey {
        case conversations, selectedConversationID, contacts
    }

    init(from decoder: Decoder) throws {
        let values = try decoder.container(keyedBy: CodingKeys.self)
        conversations = try values.decode([LinksMacOSConversation].self, forKey: .conversations)
        selectedConversationID = try values.decodeIfPresent(
            String.self, forKey: .selectedConversationID)
        contacts = try values.decodeIfPresent([LinksMacOSContact].self, forKey: .contacts) ?? []
    }
}

@MainActor
final class LinksMacOSAppModel: ObservableObject, IOSDirectMessagingDelegate {
    private static let manualSignOutKey = "ai.links.macos.manual-sign-out.v1"
    private static let recipientPreKeyFailureMessage =
        "Recipient pre-key verification or MLS setup failed. "
        + "Check that the other profile is connected and its pre-keys are ready."
    private static let missingAccountSetupStatus = "This account no longer exists"

    @Published private(set) var identityStatus = "Checking identity"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device enrolled"
    @Published private(set) var connectionStatus = "Core not configured"
    @Published private(set) var deliveryState = LinksMacOSDeliveryState.notConfigured
    @Published private(set) var pendingOutboxCount = 0
    @Published private(set) var lifecycleStatus = "Launching"
    @Published private(set) var conversations: [LinksMacOSConversation] = []
    @Published private(set) var contacts: [LinksMacOSContact] = []
    @Published var selectedConversationID: String?
    @Published var composerText = ""
    @Published private(set) var lastError: String?
    @Published private(set) var onboardingError: String? {
        didSet {
            if let onboardingError, !onboardingError.isEmpty {
                lastError = onboardingError
            }
        }
    }
    @Published var actionError: String? {
        didSet {
            if let actionError, !actionError.isEmpty {
                lastError = actionError
            }
        }
    }
    @Published private(set) var isEnrolling = false
    @Published private(set) var isRestoringSession = false
    @Published private(set) var requiresManualSignIn = true
    @Published private(set) var profileName = ClientProfile.default.name
    @Published private(set) var profilePictureJPEG: Data?
    @Published private(set) var contactPictures: [String: Data] = [:]
    @Published private(set) var profileRootPath = ""
    @Published private(set) var profileLogPath = ""
    @Published private(set) var profileStatusPath = ""
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
    @Published private(set) var isCreatingConversation = false
    @Published private(set) var conversationCreationStatus = ""
    @Published private(set) var contactStatus = "No contacts yet"
    @Published private(set) var isAddingContact = false
    @Published var pairingTarget = ""
    @Published var pairingInput = ""
    @Published private(set) var pairingURI: String?
    @Published private(set) var pairingStatus = "No pairing activity"
    @Published private(set) var isPairing = false

    private let client: IOSClient?
    private let sessionDefaults: UserDefaults?
    private let authClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private let preKeyAPI: IOSPreKeyHTTPClient?
    private var encryptedStateStore: MacOSEncryptedStateStore?
    private var coreStateStore: MacOSEncryptedStateStore?
    private(set) var durableMessagingStore: MacOSDurableMessagingStore?
    private var coreFactory: MacOSRustCoreFactory?
    private var keychainSecretProvider: MacOSKeychainSecretProvider?
    private var otpChallenge: IOSOTPChallenge?
    private var messaging: IOSDirectMessaging?
    private var directChatDirectory: (any IOSDirectChatDirectory)?
    private var initializedConversationIDs = Set<String>()
    private var initializingConversationIDs = Set<String>()
    /// Conversations removed while MLS setup is still in flight. Late setup
    /// results for these IDs must not surface an error.
    private var discardedConnectionIDs = Set<String>()
    /// Recipient accounts the directory no longer has. Auto-setup skips them
    /// so opening the conversation does not raise a pre-key alert.
    private var unavailableRecipientUserIDs = Set<String>()
    private var resolvingIncomingUserIDs = Set<String>()
    private var profileLogger: LinksMacOSProfileLogger?
    private var profileStatus: LinksMacOSProfileStatus?
    private let identityQueue = DispatchQueue(
        label: "ai.links.macos.identity", qos: .userInitiated)
    private var connectionRequested = false
    private var reconnectAfterBackground = false
    private var preKeyMaintenanceTask: Task<Void, Never>?
    private var preKeyMaintenanceFollowUp = false
    private var isTerminating = false
    /// Set when this profile is being deleted so late tasks cannot recreate it.
    private var profileTornDown = false
    private var sendAfterSetupConversationID: String?
    private var isRefreshingContactPictures = false
    private var contactPictureRefreshTask: Task<Void, Never>?
    var requestNewProfileRegistration: ((String) -> Void)?

    init(profileOverride: ClientProfile? = nil) {
        do {
            let profile: ClientProfile
            if let profileOverride {
                profile = profileOverride
            } else {
                profile = try Self.profileFromArguments()
            }
            let (profileRoot, hasExplicitRoot) = try Self.profileRootFromArguments(profile: profile)
            profileRootPath = profileRoot.url.path
            profileLogPath = profileRoot.logsURL
                .appendingPathComponent("client.log", isDirectory: false).path
            profileStatus = try? LinksMacOSProfileStatus(root: profileRoot)
            profileStatusPath = profileStatus?.url.path
                ?? profileRoot.url.appendingPathComponent("status.json").path
            profileStatus?.write(.launching, authenticated: false, connected: false)
            profileLogger = try? LinksMacOSProfileLogger(root: profileRoot)
            encryptedStateStore = try? MacOSEncryptedStateStore(
                profile: profile,
                rootURL: profileRoot.url,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            coreStateStore = try? MacOSEncryptedStateStore(
                profile: profile,
                rootURL: profileRoot.url,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil,
                namespace: "core")
            durableMessagingStore = try? MacOSDurableMessagingStore(
                profile: profile,
                rootURL: profileRoot.url,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            let provider = MacOSKeychainSeedProvider(
                profile: profile,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            keychainSecretProvider = MacOSKeychainSecretProvider(
                profile: profile,
                keychainNamespace: hasExplicitRoot ? profileRoot.keychainNamespace : nil)
            if let coreStateStore, let keychainSecretProvider {
                coreFactory = MacOSRustCoreFactory(
                    stateStore: coreStateStore,
                    secrets: keychainSecretProvider)
            }
            let identityStore = HardwareIdentityStore(seedProvider: provider)
            let metadataDefaults = try Self.metadataDefaults(
                for: profileRoot, hasExplicitRoot: hasExplicitRoot)
            let loadedClient = try IOSClient(
                identityStore: identityStore,
                defaults: metadataDefaults,
                profile: profile)
            profileName = profile.name
            client = loadedClient
            sessionDefaults = metadataDefaults
            usernameInput = loadedClient.accountHandle ?? ""
            authMode = loadedClient.accountHandle == nil ? .register : .login
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
            let manuallySignedOut = metadataDefaults.bool(forKey: Self.manualSignOutKey)
            requiresManualSignIn = loadedClient.accountHandle == nil || manuallySignedOut
            if loadedClient.accountHandle != nil && !manuallySignedOut && authClient != nil {
                isRestoringSession = true
            }
            restoreLocalState()
            loadProfilePicture()
            loadCachedContactPictures()
            refreshClientState()
            startContactPictureRefresh()
            profileLogger?.record(.launched)
            if isRestoringSession {
                restoreSavedSession()
            }
        } catch {
            client = nil
            sessionDefaults = nil
            authClient = nil
            otpClient = nil
            preKeyAPI = nil
            encryptedStateStore = nil
            coreStateStore = nil
            durableMessagingStore = nil
            coreFactory = nil
            keychainSecretProvider = nil
            profileLogger = nil
            profileStatus?.write(.failed, authenticated: false, connected: false)
            profileStatus = nil
            profileRootPath = "Invalid profile root"
            profileLogPath = "Unavailable"
            profileStatusPath = "Unavailable"
            profileName = "Invalid profile"
            identityStatus = "Identity unavailable"
            accountStatus = "Unavailable"
            deviceStatus = "Unavailable"
            onboardingError = "Saved identity metadata could not be restored."
        }
    }

    var packageStatus: String { "LinksClient + LinksKeyStore" }

    static func profileExists(_ profile: ClientProfile) -> Bool {
        guard let (root, _) = try? Self.profileRootFromArguments(profile: profile) else {
            return true
        }
        return FileManager.default.fileExists(atPath: root.url.path)
    }

    /// `true` when the username is registered, `false` when it is gone, and `nil`
    /// when the directory could not be checked.
    func directoryAccountExists(handle: String) async -> Bool? {
        guard let authClient else { return nil }
        do {
            _ = try await authClient.lookup(handle: handle)
            return true
        } catch let authError as IOSUsernameAuthError {
            if case .serverRejected(let statusCode) = authError, statusCode == 404 {
                return false
            }
            return nil
        } catch {
            return nil
        }
    }

    /// Stop writers before deleting the profile this model has open.
    func prepareForProfileDeletion() {
        profileTornDown = true
        contactPictureRefreshTask?.cancel()
        contactPictureRefreshTask = nil
        connectionRequested = false
        reconnectAfterBackground = false
        messaging?.shutdown()
        messaging = nil
        directChatDirectory = nil
        profileLogger = nil
        profileStatus = nil
        encryptedStateStore = nil
        coreStateStore = nil
        durableMessagingStore = nil
        coreFactory = nil
        keychainSecretProvider = nil
        client?.erasePersistedIdentity()
    }

    /// Remove one local profile's identity, Keychain material, and files.
    /// Used after links-admin deletes the account so the username can be created again.
    static func removeLocalProfile(_ profile: ClientProfile) throws {
        let (root, hasExplicitRoot) = try profileRootFromArguments(profile: profile)
        let namespace = hasExplicitRoot ? root.keychainNamespace : nil
        let defaults = try metadataDefaults(for: root, hasExplicitRoot: hasExplicitRoot)
        let seedProvider = MacOSKeychainSeedProvider(
            profile: profile, keychainNamespace: namespace)
        let identityStore = HardwareIdentityStore(seedProvider: seedProvider)
        if let existing = try? IOSClient(
            identityStore: identityStore, defaults: defaults, profile: profile) {
            existing.erasePersistedIdentity()
        }
        if hasExplicitRoot || profile != .default {
            defaults.removePersistentDomain(forName: root.metadataSuiteName)
        } else {
            defaults.removeObject(forKey: "links.client.metadata.v1")
            defaults.removeObject(forKey: manualSignOutKey)
        }
        MacOSKeychainSecretProvider(
            profile: profile, keychainNamespace: namespace).eraseProfile()
        for stateNamespace in ["state", "core", "messaging"] {
            let store = try? MacOSEncryptedStateStore(
                profile: profile,
                rootURL: root.url,
                keychainNamespace: namespace,
                namespace: stateNamespace)
            store?.eraseKey()
        }
        if FileManager.default.fileExists(atPath: root.url.path) {
            try FileManager.default.removeItem(at: root.url)
        }
    }

    var requiresOnboarding: Bool { client?.isEnrolled != true }

    var requiresAccountAuthentication: Bool {
        client?.isEnrolled == true && client?.isAuthenticated != true
    }

    var shouldRestoreSavedSession: Bool {
        !requiresManualSignIn
            && client?.accountHandle != nil
            && requiresAccountAuthentication
    }

    var otpAvailable: Bool { otpClient != nil }

    var hasOTPChallenge: Bool { otpChallenge != nil }

    var selectedConversation: LinksMacOSConversation? {
        guard let selectedConversationID else { return nil }
        return conversations.first { $0.id == selectedConversationID }
    }

    func markConversationRead(_ conversationID: String) {
        guard let index = conversations.firstIndex(where: { $0.id == conversationID }),
              conversations[index].unreadCount > 0 else { return }
        conversations[index].unreadCount = 0
        persistLocalState()
    }

    var hasMessagingHost: Bool { messaging != nil }

    var isConnectionRequested: Bool { connectionRequested }

    var hasBoundAccount: Bool {
        client?.accountHandle != nil || client?.userID != nil
    }

    var canInitializeSelectedConversation: Bool {
        guard let conversation = selectedConversation, !conversation.isIncoming else {
            return false
        }
        return messaging?.state == .ready && messaging?.isConnected == true
    }

    var canComposeSelectedConversation: Bool {
        guard let conversation = selectedConversation else { return false }
        if conversation.isIncoming {
            return initializedConversationIDs.contains(conversation.id)
        }
        return true
    }

    var canSendSelectedConversation: Bool {
        guard let conversation = selectedConversation,
              !conversation.isIncoming else {
            return false
        }
        return initializedConversationIDs.contains(conversation.id)
    }

    /// Start MLS setup after a conversation is selected when all live
    /// connection and pre-key prerequisites are already ready. The explicit
    /// action remains available for retry and recovery.
    func autoInitializeSelectedConversation() {
        guard let conversation = selectedConversation else { return }
        if conversation.isIncoming || initializedConversationIDs.contains(conversation.id) {
            conversationSetupStatus = "Secure two-user MLS conversation ready"
            return
        }
        if unavailableRecipientUserIDs.contains(conversation.recipientUserID) {
            conversationSetupStatus = Self.missingAccountSetupStatus
            return
        }
        if conversationSetupStatus == Self.missingAccountSetupStatus {
            conversationSetupStatus = "MLS conversation not initialized"
        }
        guard !initializingConversationIDs.contains(conversation.id),
              messaging?.state == .ready,
              messaging?.isConnected == true,
              preKeyStatus.hasPrefix("Ready:"),
              directChatDirectory != nil,
              preKeyAPI != nil else {
            return
        }
        initializeSelectedConversation(reset: false, userInitiated: false)
    }

    func clearLastError() {
        lastError = nil
    }

    func replaceProfilePicture(with data: Data) {
        guard !profileTornDown else { return }
        guard data.count <= Self.maximumProfilePictureBytes,
              let jpeg = Self.normalizedProfilePictureJPEG(data),
              let url = profilePictureURL else {
            actionError = "Use a JPEG, PNG, or HEIC picture under 8 MB."
            return
        }
        do {
            try jpeg.write(to: url, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: NSNumber(value: 0o600)],
                ofItemAtPath: url.path)
            profilePictureJPEG = jpeg
            actionError = nil
            Task { await self.publishProfilePicture() }
        } catch {
            actionError = "The profile picture could not be saved."
        }
    }

    func removeProfilePicture() {
        guard !profileTornDown, let url = profilePictureURL else { return }
        if FileManager.default.fileExists(atPath: url.path) {
            try? FileManager.default.removeItem(at: url)
        }
        profilePictureJPEG = nil
        Task { await self.deletePublishedProfilePicture() }
    }

    func pictureJPEG(for conversation: LinksMacOSConversation) -> Data? {
        let userID = conversation.peerUserID ?? conversation.recipientUserID
        if !userID.isEmpty, let picture = contactPictures[userID] {
            return picture
        }
        let handle = conversation.title.trimmingCharacters(in: CharacterSet(charactersIn: "@ "))
            .lowercased()
        guard let contact = contacts.first(where: { $0.handle == handle }) else { return nil }
        return contactPictures[contact.userID]
    }

    func refreshContactPictures() async {
        guard !profileTornDown, let authClient, !isRefreshingContactPictures else { return }
        isRefreshingContactPictures = true
        defer { isRefreshingContactPictures = false }
        var updated = contactPictures
        for contact in contacts {
            do {
                if let jpeg = try await authClient.downloadProfilePicture(handle: contact.handle) {
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
        guard profileRootPath.hasPrefix("/") else { return nil }
        return URL(fileURLWithPath: profileRootPath, isDirectory: true)
            .appendingPathComponent(Self.profilePictureFilename)
    }

    private func loadProfilePicture() {
        guard let url = profilePictureURL,
              let data = try? Data(contentsOf: url),
              data.count <= Self.maximumProfilePictureBytes,
              NSImage(data: data) != nil else {
            profilePictureJPEG = nil
            return
        }
        profilePictureJPEG = data
    }

    private func publishProfilePicture() async {
        guard !profileTornDown, let authClient, let jpeg = profilePictureJPEG,
              let token = try? client?.accessToken() else { return }
        try? await authClient.uploadProfilePicture(accessToken: token, jpeg: jpeg)
    }

    private func deletePublishedProfilePicture() async {
        guard !profileTornDown, let authClient,
              let token = try? client?.accessToken() else { return }
        try? await authClient.deleteProfilePicture(accessToken: token)
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
        guard profileRootPath.hasPrefix("/") else { return nil }
        return URL(fileURLWithPath: profileRootPath, isDirectory: true)
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
        try? FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
        let url = directory.appendingPathComponent("\(userID).jpg")
        try? jpeg.write(to: url, options: .atomic)
    }

    private func removeCachedContactPicture(userID: String) {
        guard let directory = contactPictureDirectory else { return }
        try? FileManager.default.removeItem(
            at: directory.appendingPathComponent("\(userID).jpg"))
    }

    private static let profilePictureFilename = "profile-picture.jpg"
    private static let maximumProfilePictureBytes = 8 * 1024 * 1024
    private static let profilePictureSide = 512

    private static func normalizedProfilePictureJPEG(_ data: Data) -> Data? {
        guard let image = NSImage(data: data),
              let source = image.cgImage(forProposedRect: nil, context: nil, hints: nil),
              source.width > 0, source.height > 0 else {
            return nil
        }
        let longest = max(source.width, source.height)
        let scale = min(1, CGFloat(profilePictureSide) / CGFloat(longest))
        let width = max(1, Int((CGFloat(source.width) * scale).rounded()))
        let height = max(1, Int((CGFloat(source.height) * scale).rounded()))
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else {
            return nil
        }
        context.interpolationQuality = .high
        context.draw(source, in: CGRect(x: 0, y: 0, width: width, height: height))
        guard let scaled = context.makeImage() else { return nil }
        return NSBitmapImageRep(cgImage: scaled).representation(
            using: .jpeg,
            properties: [.compressionFactor: 0.82])
    }

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
            publishProfileStatus()
        @unknown default:
            lifecycleStatus = "Unknown"
        }
    }

    func enrollIdentity() {
        enrollIdentity(completion: nil)
    }

    private func enrollIdentity(completion: (() -> Void)?) {
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
                    completion?()
                }
            } catch {
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.isEnrolling = false
                    self.onboardingError = Self.identityCreationErrorMessage(error)
                }
            }
        }
    }

    func beginUsernameRegistration(handle: String) {
        usernameInput = handle
        authMode = .register
        if client?.isEnrolled == true {
            authenticateUsername(inCurrentProfile: true)
        } else {
            enrollIdentity { [weak self] in
                self?.authenticateUsername(inCurrentProfile: true)
            }
        }
    }

    private static func identityCreationErrorMessage(_ error: Error) -> String {
        guard let identityError = error as? HardwareIdentityStore.IdentityError else {
            return "Secure identity creation failed on this Mac."
        }
        switch identityError {
        case .hardwareUnavailable, .providerFailure:
            return "Secure Enclave unavailable. Use a signed Debug build with Keychain entitlements on a physical Mac."
        case .authenticationFailed:
            return "Keychain authentication failed. Unlock this Mac and try again."
        case .invalidInput:
            return "Secure identity request was invalid. Try again."
        }
    }

    func authenticateUsername() {
        authenticateUsername(inCurrentProfile: false)
    }

    private func authenticateUsername(inCurrentProfile: Bool) {
        guard let client, let authClient, client.isEnrolled, !isAuthenticating else {
            onboardingError = "Local username auth is unavailable."
            return
        }
        let handle = usernameInput
        let mode = authMode
        if mode == .register && !inCurrentProfile {
            guard let requestNewProfileRegistration else {
                onboardingError = "Create a new profile before registering another account."
                return
            }
            requestNewProfileRegistration(handle.trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased())
            return
        }
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
                self.isRestoringSession = false
                self.requiresManualSignIn = false
                self.sessionDefaults?.set(false, forKey: Self.manualSignOutKey)
                self.usernameInput = client.accountHandle ?? handle
                self.refreshClientState()
            } catch {
                guard let self else { return }
                self.isAuthenticating = false
                self.isRestoringSession = false
                self.onboardingError = Self.usernameAuthenticationErrorMessage(error)
            }
        }
    }

    func restoreSavedSession(reconnectAfterRestore: Bool = false) {
        guard !requiresManualSignIn,
              !isAuthenticating,
              let client,
              let authClient,
              client.isEnrolled,
              let handle = client.accountHandle else {
            isRestoringSession = false
            return
        }
        if messaging != nil {
            discardMessaging()
        }
        connectionRequested = false
        isRestoringSession = true
        onboardingError = nil
        isAuthenticating = true
        Task { @MainActor [weak self] in
            do {
                _ = try await client.loginUsername(using: authClient, handle: handle)
                guard let self else { return }
                self.isAuthenticating = false
                self.isRestoringSession = false
                self.requiresManualSignIn = false
                self.sessionDefaults?.set(false, forKey: Self.manualSignOutKey)
                self.usernameInput = client.accountHandle ?? handle
                self.refreshClientState()
                if reconnectAfterRestore {
                    self.connect()
                }
            } catch {
                guard let self else { return }
                self.isAuthenticating = false
                self.isRestoringSession = false
                self.onboardingError = Self.usernameAuthenticationErrorMessage(error)
                self.refreshClientState()
            }
        }
    }

    private static func usernameAuthenticationErrorMessage(_ error: Error) -> String {
        if let clientError = error as? IOSClientError {
            switch clientError {
            case .identityReuse:
                return "This profile is already linked to another account. Use Log in or start a new profile."
            default:
                break
            }
        }
        if let authError = error as? IOSUsernameAuthError {
            switch authError {
            case .invalidHandle:
                return "Use a valid lowercase username."
            case .conflict:
                return "This device or username is already registered. Use Log in or a new profile."
            case .rateLimited(let retryAfterSeconds):
                if let retryAfterSeconds, retryAfterSeconds >= 60 {
                    let minutes = max(1, (retryAfterSeconds + 59) / 60)
                    return "Too many login attempts. Try again in about \(minutes) minutes."
                }
                return "Too many login attempts. Wait and try again."
            case .serverRejected(let statusCode) where statusCode == 401:
                return "This profile is not the device registered for that username. Open the profile that created the account."
            case .serverRejected(let statusCode) where statusCode == 404:
                return "No Links account uses that username."
            case .networkUnavailable, .cannotConnect, .timedOut:
                return "The local account service is unavailable."
            case .tlsRejected:
                return "The secure connection to the account service was rejected."
            default:
                break
            }
        }
        return "Username auth failed. Check the handle and local auth service."
    }

    private static func connectionStartErrorMessage(_ error: Error) -> String {
        if let coreError = error as? MacOSRustCoreError {
            switch coreError {
            case .status(let status):
                switch status {
                case 1:
                    return "Rust core rejected profile data (code 1). Log in again."
                case 2:
                    return "Crypto provider unavailable (code 2). Unlock this Mac and try again."
                case 3:
                    return "Identity and MLS credential do not match (code 3). Log in again."
                case 4:
                    return "Profile state or Keychain provider failed (code 4)."
                case 5:
                    return "Saved mailbox cursor is stale (code 5). Use recovery."
                default:
                    return "Rust core could not start (code \(status))."
                }
            }
        }
        if error is IOSConnectionError {
            return "Gateway endpoint is invalid. Use ws://127.0.0.1:8081/v1/connect in Debug."
        }
        if let clientError = error as? IOSClientError {
            switch clientError {
            case .authenticatedSessionRequired, .metadataUnavailable:
                return "Account session metadata is incomplete. Log in again."
            case .coreIdentityMismatch, .identityReuse:
                return "This profile identity does not match its account. Use the correct profile."
            default:
                break
            }
        }
        return "Connection could not start. Check the local gateway and profile state."
    }

    private static func preKeySetupErrorMessage(_ error: Error) -> String {
        if let coreError = error as? MacOSRustCoreError {
            switch coreError {
            case .status(let status) where status == 3:
                return "Pre-key Keychain storage unavailable. Use a signed Debug build."
            case .status(let status) where status == 4:
                return "Pre-key profile state could not be saved. Check Keychain access."
            case .status(let status):
                return "Pre-key setup failed (code \(status))."
            }
        }
        if let error = error as? IOSPreKeyError {
            switch error {
            case .serviceRejected:
                return "Pre-key service rejected the request. Check the local account service."
            case .conflict:
                return "Pre-key profile is already published."
            case .invalidToken:
                return "Pre-key upload needs a fresh account login."
            case .invalidUpload:
                return "Pre-key upload was rejected before sending. Rebuild the signed Debug app."
            case .invalidResponse:
                return "Pre-key service returned an invalid response. Restart the local backend."
            case .invalidEndpoint:
                return "Pre-key endpoint is invalid. Use http://127.0.0.1:8080 in Debug."
            case .invalidRecipient:
                return "Pre-key recipient data is invalid. Check the account device record."
            }
        }
        if let error = error as? IOSMessagingError {
            switch error {
            case .preKeyBootstrapUnavailable:
                return "The shared Rust core cannot generate pre-keys."
            default:
                break
            }
        }
        return "Initial pre-key inventory could not be uploaded (\(String(describing: error)))."
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
                self.isRestoringSession = false
                self.requiresManualSignIn = false
                self.sessionDefaults?.set(false, forKey: Self.manualSignOutKey)
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
        connectionRequested = true
        reconnectAfterBackground = false
        connectionStatus = LinksMacOSDeliveryState.connecting.title
        deliveryState = .connecting
        pendingOutboxCount = 0
        actionError = nil
        publishProfileStatus()
        connect()
    }

    func connect() {
        guard let messaging else {
            connectionStatus = "Core not configured"
            deliveryState = .notConfigured
            actionError = "Messaging host is not configured yet."
            publishProfileStatus()
            return
        }
        connectionRequested = true
        deliveryState = .connecting
        connectionStatus = deliveryState.title
        publishProfileStatus()
        do {
            try messaging.start()
            profileLogger?.record(.connectionStarted)
            pendingOutboxCount = messaging.pendingOutboxCount
            actionError = nil
            schedulePreKeyMaintenance()
        } catch {
            profileLogger?.record(.connectionFailed)
            connectionRequested = false
            deliveryState = .dependencyOutage
            connectionStatus = deliveryState.title
            actionError = Self.connectionStartErrorMessage(error)
            publishProfileStatus()
        }
    }

    /// Connect can be entered again while the first replenishment is still
    /// uploading. A second in-flight upload of the same shortfall used to hit
    /// the server pool cap and stick the pre-key banner on screen.
    private func schedulePreKeyMaintenance() {
        guard preKeyAPI != nil, messaging != nil else { return }
        if preKeyMaintenanceTask != nil {
            preKeyMaintenanceFollowUp = true
            return
        }
        preKeyStatus = "Preparing pre-key inventory"
        preKeyMaintenanceTask = Task { @MainActor [weak self] in
            await self?.runPreKeyMaintenance()
            guard let self else { return }
            self.preKeyMaintenanceTask = nil
            if self.preKeyMaintenanceFollowUp {
                self.preKeyMaintenanceFollowUp = false
                if self.connectionRequested, self.messaging != nil {
                    self.schedulePreKeyMaintenance()
                }
            }
        }
    }

    private func runPreKeyMaintenance() async {
        guard let messaging, let preKeyAPI, connectionRequested else { return }
        do {
            let inventory = try await messaging.maintainPreKeyInventory(using: preKeyAPI)
            guard self.messaging === messaging, connectionRequested else { return }
            preKeyStatus = "Ready: \(inventory.oneTimeCurvePreKeys) curve, "
                + "\(inventory.oneTimeKEMPreKeys) KEM keys"
            clearPreKeyRejection()
            publishProfileStatus()
            autoInitializeSelectedConversation()
        } catch IOSPreKeyError.conflict {
            guard self.messaging === messaging, connectionRequested else { return }
            preKeyStatus = "Ready: pre-key profile already published"
            clearPreKeyRejection()
            publishProfileStatus()
            autoInitializeSelectedConversation()
        } catch {
            guard self.messaging === messaging, connectionRequested else { return }
            let message = Self.preKeySetupErrorMessage(error)
            preKeyStatus = message
            actionError = message
            publishProfileStatus()
        }
    }

    private func clearPreKeyRejection() {
        let rejected = Self.preKeySetupErrorMessage(IOSPreKeyError.serviceRejected)
        if actionError == rejected {
            actionError = nil
        }
        if lastError == rejected {
            clearLastError()
        }
    }

    func disconnect() {
        connectionRequested = false
        preKeyMaintenanceFollowUp = false
        reconnectAfterBackground = false
        messaging?.stop()
        profileLogger?.record(.connectionStopped)
        deliveryState = .offline
        connectionStatus = deliveryState.title
        publishProfileStatus()
    }

    /// Sign out of the account while preserving this profile's identity and
    /// encrypted local state. The bearer session is memory-only, so clearing
    /// it returns the app to account authentication without deleting data.
    func logout() {
        let signingOutClient = client
        let accessToken = signingOutClient.flatMap { try? $0.accessToken() }
        sessionDefaults?.set(true, forKey: Self.manualSignOutKey)
        requiresManualSignIn = true
        isRestoringSession = false
        authMode = .login
        usernameInput = signingOutClient?.accountHandle ?? usernameInput
        disconnect()
        discardMessaging()
        initializedConversationIDs.removeAll()
        initializingConversationIDs.removeAll()
        conversationSetupStatus = "Signing out…"
        actionError = nil
        clearLastError()

        Task { @MainActor [weak self] in
            var remoteFailure = accessToken != nil
            if let accessToken, let authClient = self?.authClient {
                do {
                    try await authClient.logout(accessToken: accessToken)
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
            self.conversationSetupStatus = "Sign in to reconnect"
            self.actionError = remoteFailure
                ? "Signed out locally, but remote session revocation could not be confirmed."
                : nil
            self.refreshClientState()
        }
    }

    /// Remove only the selected local chat connection. The saved contact,
    /// account identity, Keychain seed, and other conversations stay intact.
    /// A later click on the contact creates a fresh conversation ID.
    func removeSelectedConnection() {
        guard let conversationID = selectedConversationID,
              let conversation = conversations.first(where: {
                  $0.id == conversationID
              }) else {
            actionError = "Select a conversation to remove."
            return
        }
        discardedConnectionIDs.insert(conversation.id)
        initializingConversationIDs.remove(conversation.id)
        conversations.removeAll { $0.id == conversation.id }
        initializedConversationIDs.remove(conversation.id)
        if !conversations.contains(where: { $0.recipientUserID == conversation.recipientUserID }) {
            unavailableRecipientUserIDs.remove(conversation.recipientUserID)
        }
        selectedConversationID = conversations.first?.id
        conversationSetupStatus = "MLS conversation not initialized"
        actionError = nil
        if lastError == Self.recipientPreKeyFailureMessage {
            clearLastError()
        }
        persistLocalState()
    }

    func recoverStaleCursor() {
        guard let messaging else {
            deliveryState = .notConfigured
            connectionStatus = deliveryState.title
            publishProfileStatus()
            return
        }
        guard deliveryState == .staleCursor else { return }
        deliveryState = .connecting
        connectionStatus = deliveryState.title
        publishProfileStatus()
        do {
            try messaging.recoverFromStaleCursor()
            actionError = nil
        } catch {
            deliveryState = .staleCursor
            connectionStatus = deliveryState.title
            actionError = "Full mailbox recovery is unavailable on this host."
            publishProfileStatus()
        }
    }

    /// Stop transport before process termination. The shared core owns the
    /// durable outbox and cursor, so shutdown releases transport state without
    /// replacing or clearing pending encrypted work.
    func shutdownForTermination() {
        connectionRequested = false
        reconnectAfterBackground = false
        isTerminating = true
        lifecycleStatus = "Stopping"
        profileLogger?.record(.stopping)
        messaging?.shutdown()
        deliveryState = .offline
        connectionStatus = deliveryState.title
        publishProfileStatus()
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
            peerUserID: cleanRecipient,
            messages: [])
        conversations.append(conversation)
        selectedConversationID = conversation.id
        actionError = nil
        persistLocalState()
        return true
    }

    func createConversation(handle: String) async -> Bool {
        guard let authClient, let client, client.isAuthenticated,
              !isCreatingConversation else {
            conversationCreationStatus = "Sign in before starting a conversation."
            return false
        }
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .replacingOccurrences(of: "^@", with: "", options: .regularExpression)
        do {
            try IOSUsernameAuthClient.validateHandle(cleanHandle)
        } catch {
            conversationCreationStatus = "Use 3–32 lowercase letters, numbers, or underscores."
            return false
        }

        isCreatingConversation = true
        conversationCreationStatus = "Finding @\(cleanHandle)…"
        defer { isCreatingConversation = false }
        do {
            let directory = try await authClient.lookup(handle: cleanHandle)
            guard directory.userID != client.userID else {
                conversationCreationStatus = "Choose someone other than your own account."
                return false
            }
            guard !directory.devices.isEmpty else {
                conversationCreationStatus = "@\(directory.handle) has no active devices."
                return false
            }

            let contact = LinksMacOSContact(
                handle: directory.handle,
                userID: directory.userID,
                deviceCount: directory.devices.count)
            if let contactIndex = contacts.firstIndex(where: { $0.userID == contact.userID }) {
                contacts[contactIndex] = contact
            } else {
                contacts.append(contact)
                contacts.sort { $0.handle < $1.handle }
            }
            startConversation(with: contact)
            conversationCreationStatus = "Opened @\(directory.handle)."
            return true
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
                conversationCreationStatus = "Could not find @\(cleanHandle)."
            }
            return false
        } catch {
            conversationCreationStatus = "Could not find @\(cleanHandle)."
            return false
        }
    }

    func addContact(handle: String) async -> Bool {
        guard let authClient, client?.isAuthenticated == true, !isAddingContact else {
            contactStatus = "Sign in before adding contacts"
            return false
        }
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased().replacingOccurrences(of: "^@", with: "", options: .regularExpression)
        guard !cleanHandle.isEmpty else {
            contactStatus = "Enter a username"
            return false
        }
        isAddingContact = true
        contactStatus = "Looking up contact"
        defer { isAddingContact = false }
        do {
            let directory = try await authClient.lookup(handle: cleanHandle)
            guard directory.userID != client?.userID else {
                throw IOSUsernameAuthError.invalidHandle
            }
            let contact = LinksMacOSContact(
                handle: directory.handle,
                userID: directory.userID,
                deviceCount: directory.devices.count)
            if let index = contacts.firstIndex(where: { $0.userID == contact.userID }) {
                contacts[index] = contact
            } else {
                contacts.append(contact)
            }
            contacts.sort { $0.handle < $1.handle }
            persistLocalState()
            contactStatus = "Added @\(contact.handle)"
            return true
        } catch {
            contactStatus = "Contact not found. Use a valid lowercase username."
            return false
        }
    }

    func removeContact(_ contact: LinksMacOSContact) {
        contacts.removeAll { $0.userID == contact.userID }
        contactStatus = "Removed @\(contact.handle)"
        persistLocalState()
    }

    func startConversation(with contact: LinksMacOSContact) {
        if let index = conversations.firstIndex(where: {
            $0.peerUserID == contact.userID
                || (!$0.isIncoming && $0.recipientUserID == contact.userID)
        }) {
            if conversations[index].isIncoming {
                conversations[index] = LinksMacOSConversation(
                    id: conversations[index].id,
                    title: "@\(contact.handle)",
                    recipientUserID: contact.userID,
                    peerUserID: contact.userID,
                    messages: conversations[index].messages,
                    unreadCount: conversations[index].unreadCount)
            }
            selectedConversationID = conversations[index].id
            markConversationRead(conversations[index].id)
            persistLocalState()
            return
        }
        _ = createConversation(title: "@\(contact.handle)", recipientUserID: contact.userID)
    }

    /// Claim and verify recipient pre-keys, then stage the first two-user MLS
    /// conversation in the shared client core.
    func initializeSelectedConversation() {
        forgetUnavailableRecipientForSelectedConversation()
        initializeSelectedConversation(reset: false, userInitiated: true)
    }

    /// Recreate a broken direct MLS group and release the recipient's stuck
    /// mailbox batch after the fresh bootstrap arrives.
    func resetSelectedConversation() {
        forgetUnavailableRecipientForSelectedConversation()
        initializeSelectedConversation(reset: true, userInitiated: true)
    }

    /// Automatic setup (opening a chat, sending before it is ready) reports
    /// failures inline; only the explicit buttons raise an alert.
    private func initializeSelectedConversation(reset: Bool, userInitiated: Bool) {
        guard let selectedConversationID,
              let conversation = conversations.first(where: {
                  $0.id == selectedConversationID
              }),
              let messaging else {
            conversationSetupStatus = "MLS host directory is not configured"
            actionError = "Configure an authenticated directory with MLS KeyPackages first."
            return
        }
        guard !conversation.isIncoming else {
            conversationSetupStatus = "Secure two-user MLS conversation ready"
            actionError = nil
            clearLastError()
            return
        }
        guard messaging.state == .ready && messaging.isConnected else {
            conversationSetupStatus = "Connect before initializing MLS"
            actionError = "Click Connect and wait for Ready, then initialize secure chat."
            return
        }
        guard let preKeyAPI, let directChatDirectory else {
            conversationSetupStatus = "MLS host directory is not configured"
            actionError = "Configure an authenticated directory with MLS KeyPackages first."
            return
        }
        guard preKeyStatus.hasPrefix("Ready:") else {
            conversationSetupStatus = "Pre-key setup is not ready"
            actionError = preKeyStatus == "Preparing pre-key inventory"
                ? "Wait for pre-key setup to finish."
                : preKeyStatus
            return
        }
        guard initializingConversationIDs.insert(conversation.id).inserted else {
            return
        }
        let conversationID = conversation.id
        let recipientUserID = conversation.recipientUserID
        initializedConversationIDs.remove(conversationID)
        discardedConnectionIDs.remove(conversationID)
        conversationSetupStatus = reset
            ? "Repairing secure chat"
            : "Claiming recipient pre-keys"
        Task { @MainActor [weak self] in
            guard let self else { return }
            defer {
                self.initializingConversationIDs.remove(conversationID)
                self.discardedConnectionIDs.remove(conversationID)
            }
            do {
                if reset {
                    try await messaging.resetFirstDirectConversation(
                        conversationID: conversationID,
                        recipientUserID: recipientUserID,
                        directory: directChatDirectory,
                        preKeyAPI: preKeyAPI)
                } else {
                    try await messaging.initializeFirstDirectConversation(
                        conversationID: conversationID,
                        recipientUserID: recipientUserID,
                        directory: directChatDirectory,
                        preKeyAPI: preKeyAPI)
                }
                self.finishConversationSetup(conversationID: conversationID)
            } catch {
                self.reportConversationSetupFailure(
                    error,
                    conversationID: conversationID,
                    recipientUserID: recipientUserID,
                    reset: reset,
                    userInitiated: userInitiated)
            }
        }
    }

    private func forgetUnavailableRecipientForSelectedConversation() {
        guard let recipientUserID = selectedConversation?.recipientUserID else { return }
        unavailableRecipientUserIDs.remove(recipientUserID)
    }

    private func finishConversationSetup(conversationID: String) {
        guard conversationSetupStillCurrent(conversationID) else { return }
        initializedConversationIDs.insert(conversationID)
        if let recipientUserID = conversations.first(where: {
            $0.id == conversationID
        })?.recipientUserID {
            unavailableRecipientUserIDs.remove(recipientUserID)
        }
        guard selectedConversationID == conversationID else { return }
        conversationSetupStatus = "Secure two-user MLS conversation ready"
        if messaging?.state == .ready {
            deliveryState = .ready
            connectionStatus = deliveryState.title
        }
        actionError = nil
        publishProfileStatus()
        if sendAfterSetupConversationID == conversationID {
            sendAfterSetupConversationID = nil
            if !composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                sendMessage()
            }
        }
    }

    private func reportConversationSetupFailure(
        _ error: Error,
        conversationID: String,
        recipientUserID: String,
        reset: Bool,
        userInitiated: Bool
    ) {
        guard conversationSetupStillCurrent(conversationID) else { return }
        if sendAfterSetupConversationID == conversationID {
            sendAfterSetupConversationID = nil
        }
        if Self.isMissingRecipientAccount(error) {
            unavailableRecipientUserIDs.insert(recipientUserID)
            guard selectedConversationID == conversationID else { return }
            conversationSetupStatus = Self.missingAccountSetupStatus
            actionError = nil
            if lastError == Self.recipientPreKeyFailureMessage {
                clearLastError()
            }
            publishProfileStatus()
            return
        }
        guard selectedConversationID == conversationID else { return }
        if let messagingError = error as? IOSMessagingError,
           case .notConnected = messagingError {
            conversationSetupStatus = "Connect before initializing MLS"
            actionError = "Connection dropped. Click Connect and try again."
        } else if userInitiated {
            conversationSetupStatus = reset
                ? "Secure chat repair failed"
                : "MLS conversation setup failed"
            actionError = Self.recipientPreKeyFailureMessage
        } else {
            conversationSetupStatus =
                "Secure chat is not ready yet. Ask the other person to open Links, then open this chat again."
            if lastError == Self.recipientPreKeyFailureMessage {
                clearLastError()
            }
        }
        publishProfileStatus()
    }

    private func conversationSetupStillCurrent(_ conversationID: String) -> Bool {
        !discardedConnectionIDs.contains(conversationID)
            && conversations.contains { $0.id == conversationID }
    }

    private static func isMissingRecipientAccount(_ error: Error) -> Bool {
        if let directoryError = error as? MacOSDirectoryError,
           case .accountNotFound = directoryError {
            return true
        }
        if let authError = error as? IOSUsernameAuthError,
           case .serverRejected(let statusCode) = authError,
           statusCode == 404 {
            return true
        }
        return false
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
        guard initializedConversationIDs.contains(conversation.id) else {
            sendAfterSetupConversationID = conversation.id
            conversationSetupStatus = "Starting secure chat. Your message will send when it is ready."
            if !initializingConversationIDs.contains(conversation.id) {
                forgetUnavailableRecipientForSelectedConversation()
                initializeSelectedConversation(reset: false, userInitiated: false)
            }
            return
        }
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
            publishProfileStatus()
        }
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                    didChange state: IOSDirectMessaging.State) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.handleMessagingStateChange(state, messaging: messaging)
        }
    }

    private func handleMessagingStateChange(_ state: IOSDirectMessaging.State,
                                            messaging: IOSDirectMessaging) {
        guard self.messaging === messaging else { return }
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
            let reconnectAfterRestore = connectionRequested
            deliveryState = .authenticationExpired
            discardMessaging()
            client?.clearAuthenticatedSession()
            connectionRequested = false
            restoreSavedSession(reconnectAfterRestore: reconnectAfterRestore)
        case .dependencyOutage:
            deliveryState = .dependencyOutage
        case .sendFailed:
            deliveryState = .sendFailed
        case .failed:
            deliveryState = .dependencyOutage
        }
        connectionStatus = deliveryState.title
        publishProfileStatus()
        if state == .ready {
            autoInitializeSelectedConversation()
        }
    }

    nonisolated func directMessaging(_ messaging: IOSDirectMessaging,
                                    didReceive message: IOSReceivedTextMessage) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.renderReceivedMessage(message)
        }
    }

    private func renderReceivedMessage(_ message: IOSReceivedTextMessage) {
        let received = LinksMacOSMessage(
            id: UUID(),
            text: message.text,
            isOutgoing: false,
            sentAt: Date(timeIntervalSince1970: TimeInterval(message.sentAtMs) / 1000),
            senderDeviceID: message.senderDeviceID)
        let knownContact = contacts.first(where: { $0.userID == message.senderUserID })
        let index = conversations.firstIndex(where: { $0.id == message.conversationID })
            ?? conversations.firstIndex(where: {
                $0.peerUserID == message.senderUserID
                    || (!$0.isIncoming && $0.recipientUserID == message.senderUserID)
            })
        let conversationID: String
        if let index {
            conversations[index].recipientUserID = message.senderUserID
            conversations[index].peerUserID = message.senderUserID
            if let knownContact {
                conversations[index].title = "@\(knownContact.handle)"
            }
            conversations[index].messages.append(received)
            if selectedConversationID != conversations[index].id
                || lifecycleStatus != "Active" {
                conversations[index].unreadCount += 1
            }
            conversationID = conversations[index].id
        } else {
            conversations.append(LinksMacOSConversation(
                id: message.conversationID,
                title: knownContact.map { "@\($0.handle)" } ?? "Incoming conversation",
                recipientUserID: message.senderUserID,
                peerUserID: message.senderUserID,
                messages: [received],
                unreadCount: 1))
            conversationID = message.conversationID
        }
        initializedConversationIDs.insert(conversationID)
        conversationSetupStatus = "Secure two-user MLS conversation ready"
        actionError = nil
        clearLastError()
        persistLocalState()
        if knownContact == nil {
            resolveIncomingUsername(
                conversationID: conversationID,
                senderUserID: message.senderUserID)
        }
    }

    private func resolveIncomingUsername(conversationID: String, senderUserID: String) {
        guard !resolvingIncomingUserIDs.contains(senderUserID),
              let authClient,
              let client,
              let accessToken = try? client.accessToken() else { return }
        resolvingIncomingUserIDs.insert(senderUserID)
        Task { @MainActor [weak self] in
            guard let self else { return }
            defer { self.resolvingIncomingUserIDs.remove(senderUserID) }
            do {
                let directory = try await authClient.lookup(
                    userID: senderUserID, accessToken: accessToken)
                guard let index = self.conversations.firstIndex(where: {
                    $0.id == conversationID && $0.peerUserID == senderUserID
                }) else { return }
                self.conversations[index].title = "@\(directory.handle)"
                self.conversations[index].recipientUserID = senderUserID
                self.initializedConversationIDs.insert(conversationID)
                if let contactIndex = self.contacts.firstIndex(where: {
                    $0.userID == senderUserID
                }) {
                    self.contacts[contactIndex] = LinksMacOSContact(
                        handle: directory.handle,
                        userID: directory.userID,
                        deviceCount: directory.devices.count)
                } else {
                    self.contacts.append(LinksMacOSContact(
                        handle: directory.handle,
                        userID: directory.userID,
                        deviceCount: directory.devices.count))
                }
                self.contacts.sort { $0.handle < $1.handle }
                self.persistLocalState()
            } catch {
                // Keep the non-identifying placeholder; message receipt still succeeds.
            }
        }
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.handleMessagingFailure(.dependencyOutage, messaging: messaging)
        }
    }

    nonisolated func directMessagingDidFail(_ messaging: IOSDirectMessaging,
                                            reason: IOSMessagingIssue) {
        Task { @MainActor [weak self] in
            guard let self, self.messaging === messaging else { return }
            self.handleMessagingFailure(reason, messaging: messaging)
        }
    }

    private func handleMessagingFailure(_ reason: IOSMessagingIssue,
                                        messaging: IOSDirectMessaging) {
        guard self.messaging === messaging else { return }
        pendingOutboxCount = messaging.pendingOutboxCount
        switch reason {
        case .staleCursor:
            deliveryState = .staleCursor
        case .authenticationExpired:
            let reconnectAfterRestore = connectionRequested
            deliveryState = .authenticationExpired
            discardMessaging()
            client?.clearAuthenticatedSession()
            connectionRequested = false
            restoreSavedSession(reconnectAfterRestore: reconnectAfterRestore)
        case .dependencyOutage:
            deliveryState = .dependencyOutage
        case .sendFailed:
            deliveryState = pendingOutboxCount > 0
                ? .offlineOutboxRetry(count: pendingOutboxCount) : .sendFailed
        }
        connectionStatus = deliveryState.title
        actionError = deliveryState.detail
        publishProfileStatus()
    }

    private func discardMessaging() {
        let previous = messaging
        messaging = nil
        directChatDirectory = nil
        previous?.shutdown()
    }

    private func refreshClientState() {
        guard !profileTornDown, let client else { return }
        identityStatus = client.isEnrolled ? "Identity enrolled" : "Identity not enrolled"
        if let handle = client.accountHandle {
            accountStatus = client.isAuthenticated
                ? "@\(handle) · Authenticated"
                : "@\(handle) · Sign in required"
        } else {
            accountStatus = client.isAuthenticated ? "Authenticated" : "Signed out"
        }
        if let deviceID = client.deviceID, let mlsNodeID = client.mlsNodeID {
            deviceStatus = "Device \(shortID(deviceID)) · Node \(shortID(mlsNodeID))"
        } else {
            deviceStatus = "No device enrolled"
        }
        configureMessagingIfPossible()
        if client.isAuthenticated {
            let unresolved = conversations.compactMap { conversation -> (String, String)? in
                guard let peerUserID = conversation.peerUserID,
                      contacts.contains(where: { $0.userID == peerUserID }) == false else {
                    return nil
                }
                return (conversation.id, peerUserID)
            }
            for (conversationID, peerUserID) in unresolved {
                resolveIncomingUsername(
                    conversationID: conversationID,
                    senderUserID: peerUserID)
            }
        }
        publishProfileStatus()
    }

    private func configureMessagingIfPossible() {
        guard !profileTornDown,
              messaging == nil,
              let client,
              client.isAuthenticated,
              let factory = coreFactory,
              let authClient,
              let preKeyAPI,
              let endpoint = try? Self.gatewayEndpointFromArguments() else {
            return
        }
        let keyPackageProvider = IOSHTTPMLSKeyPackageProvider(api: preKeyAPI)
        let directory = MacOSDirectoryChatAdapter(
            directoryClient: authClient,
            keyPackageProvider: keyPackageProvider) { [weak self] in
                guard let self else { return [:] }
                return Dictionary(uniqueKeysWithValues: self.contacts.map {
                    ($0.userID, $0.handle)
                })
            }
        do {
            try installMessaging(factory: factory, endpoint: endpoint, directory: directory)
        } catch {
            connectionStatus = "Core unavailable"
            deliveryState = .dependencyOutage
            actionError = "Shared Rust core could not be opened for this profile."
        }
    }

    private func publishProfileStatus() {
        guard !profileTornDown, let profileStatus else { return }
        let authenticated = client?.isAuthenticated == true
        let connected = messaging?.isConnected == true
        let state: LinksMacOSProfileStatus.State
        if isTerminating {
            state = .stopped
        } else if client == nil {
            state = .failed
        } else if client?.isEnrolled != true {
            state = .identityRequired
        } else if !authenticated {
            state = .authenticationRequired
        } else {
            switch deliveryState {
            case .notConfigured: state = .notConfigured
            case .offline: state = .offline
            case .connecting: state = .connecting
            case .ready: state = connected ? .ready : .connecting
            case .reconnecting: state = .reconnecting
            case .offlineOutboxRetry: state = .retryingOutbox
            case .staleCursor: state = .staleCursor
            case .authenticationExpired: state = .authenticationExpired
            case .sendFailed: state = .sendFailed
            case .dependencyOutage: state = .dependencyOutage
            }
        }
        profileStatus.write(state, authenticated: authenticated, connected: connected)
    }

    private func restoreLocalState() {
        guard let encryptedStateStore else { return }
        do {
            guard let encoded = try encryptedStateStore.read() else { return }
            let state = try PropertyListDecoder().decode(
                LinksMacOSPersistedState.self, from: encoded)
            guard state.conversations.count <= 5_000,
                  state.conversations.allSatisfy(isValidConversation),
                  state.contacts.count <= 5_000,
                  state.contacts.allSatisfy(isValidContact) else {
                throw MacOSEncryptedStateStore.StateError.invalidState
            }
            conversations = state.conversations
            contacts = state.contacts
            selectedConversationID = state.selectedConversationID.flatMap { selectedID in
                state.conversations.contains(where: { $0.id == selectedID }) ? selectedID : nil
            }
            if let selectedConversation,
               selectedConversation.isIncoming,
               !selectedConversation.messages.isEmpty {
                conversationSetupStatus = "Secure two-user MLS conversation ready"
            }
        } catch {
            conversations = []
            selectedConversationID = nil
            actionError = "Encrypted local state could not be restored."
        }
    }

    private func persistLocalState() {
        guard !profileTornDown, let encryptedStateStore else { return }
        let state = LinksMacOSPersistedState(
            conversations: conversations, selectedConversationID: selectedConversationID,
            contacts: contacts)
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
              conversation.peerUserID.map(IOSClient.isCanonicalUUID) ?? true,
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

    private func isValidContact(_ contact: LinksMacOSContact) -> Bool {
        IOSClient.isCanonicalUUID(contact.userID)
            && (try? IOSUsernameAuthClient.validateHandle(contact.handle)) != nil
            && (0...100).contains(contact.deviceCount)
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

    private static func gatewayEndpointFromArguments() throws -> URL {
        let arguments = CommandLine.arguments
        let raw: String
        if let marker = arguments.firstIndex(of: "--gateway-url"),
           arguments.index(after: marker) < arguments.endIndex {
            raw = arguments[arguments.index(after: marker)]
        } else {
            raw = ProcessInfo.processInfo.environment["LINKS_GATEWAY_ENDPOINT"]
                ?? "ws://127.0.0.1:8081/v1/connect"
        }
        guard let endpoint = URL(string: raw) else {
            throw IOSConnectionError.invalidEndpoint
        }
        return endpoint
    }
}

@MainActor
final class LinksMacOSProfileSession: ObservableObject {
    @Published private(set) var model: LinksMacOSAppModel

    init() {
        model = LinksMacOSAppModel()
        attachProfileRegistrationHandler()
    }

    private func attachProfileRegistrationHandler() {
        model.requestNewProfileRegistration = { [weak self] handle in
            self?.createProfileAndRegister(handle: handle)
        }
    }

    private func createProfileAndRegister(handle: String) {
        let cleanHandle = handle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard (try? IOSUsernameAuthClient.validateHandle(cleanHandle)) != nil,
              let profile = try? ClientProfile(name: cleanHandle) else {
            model.actionError = "Use a valid lowercase username."
            return
        }

        if profile.name == model.profileName && !model.hasBoundAccount
            && !LinksMacOSAppModel.profileExists(profile) {
            model.beginUsernameRegistration(handle: cleanHandle)
            return
        }

        guard LinksMacOSAppModel.profileExists(profile) else {
            replaceModel(with: profile, handle: cleanHandle, deletingCurrent: false)
            return
        }

        Task { @MainActor in
            await self.registerIfAccountWasDeleted(profile, handle: cleanHandle)
        }
    }

    private func registerIfAccountWasDeleted(_ profile: ClientProfile, handle: String) async {
        guard await model.directoryAccountExists(handle: handle) == false else {
            model.actionError = "The profile \(profile.name) already exists. Use that profile to log in."
            return
        }
        let deletingCurrent = profile.name == model.profileName
        if deletingCurrent {
            model.prepareForProfileDeletion()
        }
        do {
            try LinksMacOSAppModel.removeLocalProfile(profile)
        } catch {
            model.actionError = "The saved profile \(profile.name) could not be removed."
            return
        }
        replaceModel(with: profile, handle: handle, deletingCurrent: deletingCurrent)
    }

    private func replaceModel(with profile: ClientProfile,
                              handle: String,
                              deletingCurrent: Bool) {
        if !deletingCurrent {
            model.logout()
        }
        let newModel = LinksMacOSAppModel(profileOverride: profile)
        model = newModel
        attachProfileRegistrationHandler()
        newModel.beginUsernameRegistration(handle: handle)
    }
}
