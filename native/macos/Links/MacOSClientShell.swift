import SwiftUI
import LinksClient

struct LinksRootView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if model.requiresOnboarding {
                    LinksOnboardingView(model: model)
                } else if model.requiresAccountAuthentication {
                    LinksAccountOnboardingView(model: model)
                } else {
                    LinksMessagingView(model: model)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let error = model.lastError {
                PersistentErrorBanner(error: error) {
                    model.clearLastError()
                }
            }
        }
        .alert("Links", isPresented: actionErrorBinding) {
            Button("OK", role: .cancel) {}
        } message: {
            Text(model.actionError ?? "")
        }
    }

    private var actionErrorBinding: Binding<Bool> {
        Binding(
            get: { model.actionError != nil },
            set: { if !$0 { model.actionError = nil } })
    }
}

private struct PersistentErrorBanner: View {
    let error: String
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill")
                .font(.title3)
                .foregroundStyle(.red)
            VStack(alignment: .leading, spacing: 3) {
                Text("Needs attention")
                    .font(.callout.weight(.semibold))
                Text(error)
                    .font(.caption)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            Spacer(minLength: 12)
            Button("Dismiss", action: dismiss)
                .buttonStyle(.bordered)
                .controlSize(.small)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.regularMaterial)
        .background(Color.red.opacity(0.08))
        .overlay(alignment: .top) {
            Divider()
                .overlay(.red.opacity(0.35))
        }
    }
}

private struct LinksOnboardingView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(spacing: 22) {
            Image(systemName: "lock.shield")
                .font(.system(size: 56))
                .foregroundStyle(.tint)
            Text("Set up Links")
                .font(.largeTitle.weight(.semibold))
            Text("Create a hardware-backed identity for this Mac. The seed stays inside Apple identity custody.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 480)
            Text(model.identityStatus)
                .font(.headline)
            if model.isEnrolling {
                ProgressView("Creating secure identity")
            } else {
                Button("Create secure identity") {
                    model.enrollIdentity()
                }
                .buttonStyle(.borderedProminent)
            }
            if let error = model.onboardingError {
                Text(error)
                    .foregroundStyle(.red)
                    .multilineTextAlignment(.center)
            }
            Text("Account sign-in appears after local identity setup.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(48)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct LinksAccountOnboardingView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Image(systemName: "person.badge.key")
                    .font(.system(size: 50))
                    .foregroundStyle(.tint)
                    .frame(maxWidth: .infinity, alignment: .center)
                Text("Connect your account")
                    .font(.largeTitle.weight(.semibold))
                    .frame(maxWidth: .infinity, alignment: .center)
                Text("Register a local username or log in with the identity already enrolled on this Mac.")
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 520, alignment: .center)
                Text("Loopback development uses lowercase handles, such as alice or karine.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 520, alignment: .center)

                Picker("Account action", selection: $model.authMode) {
                    ForEach(LinksMacOSAuthMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                TextField("Username, for example alice", text: $model.usernameInput)
                    .textFieldStyle(.roundedBorder)
                    .textContentType(.username)
                Button(model.authMode == .register ? "Register username" : "Log in") {
                    model.authenticateUsername()
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.isAuthenticating)
                if model.isAuthenticating {
                    ProgressView("Contacting local account service")
                }
                Text("Auth service: \(model.authEndpointText)")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                if let error = model.onboardingError {
                    Text(error)
                        .font(.caption)
                        .foregroundStyle(.red)
                }

                Divider()
                Text("Phone account (OTP)")
                    .font(.headline)
                Text("Use this only with a real HTTPS account-auth service configured for Twilio Verify.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack {
                    TextField("Phone, for example +33123456789", text: $model.phoneInput)
                        .textFieldStyle(.roundedBorder)
                    Picker("Channel", selection: $model.otpChannel) {
                        ForEach(IOSOTPChannel.allCases, id: \.self) { channel in
                            Text(channel.rawValue.capitalized).tag(channel)
                        }
                    }
                    .labelsHidden()
                    .frame(width: 120)
                }
                Button("Send verification code") {
                    model.startOTPEnrollment()
                }
                .buttonStyle(.bordered)
                .disabled(!model.otpAvailable || model.isOTPWorking)
                if model.hasOTPChallenge {
                    HStack {
                        TextField("Verification code", text: $model.otpCodeInput)
                            .textFieldStyle(.roundedBorder)
                            .textContentType(.oneTimeCode)
                        Button("Verify") {
                            model.finishOTPEnrollment()
                        }
                        .buttonStyle(.borderedProminent)
                        .disabled(model.isOTPWorking)
                    }
                }
                Text(model.otpAvailable
                     ? model.otpStatus
                     : "Phone OTP disabled until the endpoint uses HTTPS.")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Divider()
                Text("Join an existing account")
                    .font(.headline)
                Text("Enter the account username or user ID. Scan the generated link on an authenticated device, then log in here with that username.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextField("Account username or user ID", text: $model.pairingTarget)
                    .textFieldStyle(.roundedBorder)
                Button("Create pairing link") {
                    model.createPairingLink()
                }
                .buttonStyle(.bordered)
                .disabled(model.isPairing)
                Text(model.pairingStatus)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                if let pairingURI = model.pairingURI {
                    Text(pairingURI)
                        .font(.system(.caption, design: .monospaced))
                        .textSelection(.enabled)
                        .lineLimit(4)
                    Button("Copy pairing link") {
                        model.copyPairingLink()
                    }
                    .buttonStyle(.bordered)
                }
            }
            .frame(maxWidth: 520)
            .padding(48)
            .frame(maxWidth: .infinity)
        }
    }
}

private struct LinksMessagingView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @State private var showingNewConversation = false
    @State private var showingAddContact = false
    @State private var showingPairing = false

    var body: some View {
        NavigationSplitView {
            LinksSidebar(model: model,
                         showingNewConversation: $showingNewConversation,
                         showingAddContact: $showingAddContact,
                         showingPairing: $showingPairing)
        } detail: {
            LinksConversationDetail(model: model)
        }
        .sheet(isPresented: $showingNewConversation) {
            NewConversationView(model: model)
        }
        .sheet(isPresented: $showingAddContact) {
            AddContactView(model: model)
        }
        .sheet(isPresented: $showingPairing) {
            PairingView(model: model)
        }
    }
}

private struct LinksSidebar: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var showingNewConversation: Bool
    @Binding var showingAddContact: Bool
    @Binding var showingPairing: Bool
    @State private var contactToRemove: LinksMacOSContact?

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 10) {
                ZStack {
                    RoundedRectangle(cornerRadius: 9)
                        .fill(Color.accentColor.opacity(0.16))
                    Image(systemName: "lock.shield.fill")
                        .font(.title3.weight(.semibold))
                        .foregroundStyle(.tint)
                }
                .frame(width: 32, height: 32)
                VStack(alignment: .leading, spacing: 1) {
                    Text("Links")
                        .font(.headline)
                    Text("Private messaging")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
                Button {
                    showingAddContact = true
                } label: {
                    Image(systemName: "person.badge.plus")
                }
                .buttonStyle(.borderless)
                .help("Add contact")
                Button {
                    showingNewConversation = true
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .buttonStyle(.borderless)
                .help("New conversation")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)

            List(selection: $model.selectedConversationID) {
                Section {
                    if model.conversations.isEmpty {
                        Text("No conversations yet")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.vertical, 5)
                    } else {
                        ForEach(model.conversations) { conversation in
                            ConversationRow(conversation: conversation)
                                .tag(conversation.id as String?)
                        }
                    }
                } header: {
                    SidebarSectionHeader(title: "Conversations") {
                        showingNewConversation = true
                    }
                }
                Section {
                    if model.contacts.isEmpty {
                        Text("Add someone by username")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .padding(.vertical, 5)
                    } else {
                        ForEach(model.contacts) { contact in
                            Button {
                                model.startConversation(with: contact)
                            } label: {
                                ContactRow(contact: contact)
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button("Remove Contact", role: .destructive) {
                                    contactToRemove = contact
                                }
                            }
                        }
                    }
                } header: {
                    SidebarSectionHeader(title: "Contacts") {
                        showingAddContact = true
                    }
                }
            }
            .listStyle(.sidebar)
            .scrollContentBackground(.hidden)
            .alert("Remove contact?", isPresented: Binding(
                get: { contactToRemove != nil },
                set: { isPresented in
                    if !isPresented {
                        contactToRemove = nil
                    }
                })) {
                    Button("Remove", role: .destructive) {
                        if let contact = contactToRemove {
                            model.removeContact(contact)
                        }
                        contactToRemove = nil
                    }
                    Button("Cancel", role: .cancel) {
                        contactToRemove = nil
                    }
                } message: {
                    Text("This removes the saved contact. Existing conversations and messages stay.")
                }

            Divider()
            ProfileSummaryCard(model: model, showingPairing: $showingPairing)
        }
        .frame(minWidth: 300, idealWidth: 320, maxWidth: 360)
        .background(.regularMaterial)
    }
}

private struct SidebarSectionHeader: View {
    let title: String
    let action: () -> Void

    var body: some View {
        HStack {
            Text(title)
                .font(.caption.weight(.semibold))
                .textCase(nil)
            Spacer()
            Button(action: action) {
                Image(systemName: "plus")
                    .font(.caption.weight(.bold))
            }
            .buttonStyle(.borderless)
            .help("Add to \(title.lowercased())")
        }
    }
}

private struct ProfileSummaryCard: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var showingPairing: Bool
    @State private var showingDetails = false
    @State private var logoutConfirmationPresented = false

    var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            HStack(spacing: 10) {
                ProfileAvatar(title: model.profileName, size: 34)
                VStack(alignment: .leading, spacing: 2) {
                    Text(model.profileName)
                        .font(.callout.weight(.semibold))
                        .lineLimit(1)
                    Text(model.accountStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer()
            }

            HStack(spacing: 7) {
                Circle()
                    .fill(linksStatusColor(model.connectionStatus))
                    .frame(width: 8, height: 8)
                Text(model.connectionStatus)
                    .font(.caption.weight(.medium))
                Spacer()
                if model.pendingOutboxCount > 0 {
                    Label("\(model.pendingOutboxCount) queued", systemImage: "clock.arrow.circlepath")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            HStack(spacing: 8) {
                Button {
                    if model.isConnectionRequested {
                        model.disconnect()
                    } else {
                        model.connect()
                    }
                } label: {
                    Label(model.isConnectionRequested ? "Disconnect" : "Connect",
                          systemImage: model.isConnectionRequested ? "wifi.slash" : "bolt.horizontal")
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.small)

                Button {
                    showingPairing = true
                } label: {
                    Image(systemName: "person.2.badge.plus")
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .help("Pair device")

                Spacer()
                Button("Log out") {
                    logoutConfirmationPresented = true
                }
                .buttonStyle(.borderless)
                .controlSize(.small)
                .foregroundStyle(.secondary)
            }

            DisclosureGroup("Profile details", isExpanded: $showingDetails) {
                VStack(alignment: .leading, spacing: 7) {
                    StateRow(title: "Device", value: model.deviceStatus)
                    StateRow(title: "Delivery", value: model.deliveryState.title)
                    StateRow(title: "Pre-keys", value: model.preKeyStatus)
                    StateRow(title: "Lifecycle", value: model.lifecycleStatus)
                    StateRow(title: "Data", value: model.profileRootPath)
                    StateRow(title: "Logs", value: model.profileLogPath)
                    StateRow(title: "Status", value: model.profileStatusPath)
                    Text(model.packageStatus)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                }
                .padding(.top, 7)
            }
            .font(.caption)
        }
        .padding(14)
        .confirmationDialog(
            "Log out of this profile?",
            isPresented: $logoutConfirmationPresented,
            titleVisibility: .visible) {
            Button("Log out", role: .destructive) {
                model.logout()
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Your identity, contacts, conversations, and encrypted local state stay on this Mac. You will need to sign in again to reconnect.")
        }
    }
}

private struct ProfileAvatar: View {
    let title: String
    let size: CGFloat

    var body: some View {
        Text(String(title.trimmingCharacters(in: CharacterSet(charactersIn: "@ ")).prefix(1)).uppercased())
            .font(.system(size: size * 0.42, weight: .semibold))
            .foregroundStyle(.tint)
            .frame(width: size, height: size)
            .background(Color.accentColor.opacity(0.14))
            .clipShape(Circle())
    }
}

private struct ConversationRow: View {
    let conversation: LinksMacOSConversation

    var body: some View {
        HStack(spacing: 10) {
            ProfileAvatar(title: conversation.title, size: 30)
            VStack(alignment: .leading, spacing: 3) {
                Text(conversation.title)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Text(conversation.messages.last?.text ?? "No messages")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .padding(.vertical, 3)
    }
}

private struct ContactRow: View {
    let contact: LinksMacOSContact

    var body: some View {
        HStack(spacing: 10) {
            ProfileAvatar(title: contact.handle, size: 30)
            VStack(alignment: .leading, spacing: 3) {
                Text("@\(contact.handle)")
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Text(contact.deviceCount == 1
                     ? "1 active device"
                     : "\(contact.deviceCount) active devices")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 3)
        .contentShape(Rectangle())
    }
}

private struct StateRow: View {
    let title: String
    let value: String

    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            Text(title)
                .foregroundStyle(.secondary)
            Spacer(minLength: 8)
            Text(value)
                .multilineTextAlignment(.trailing)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .font(.caption)
    }
}

private func linksStatusColor(_ value: String) -> Color {
    let normalized = value.lowercased()
    if normalized.contains("ready") || normalized.contains("connected") {
        return .green
    }
    if normalized.contains("connect") || normalized.contains("retry") || normalized.contains("pending") {
        return .orange
    }
    if normalized.contains("failed") || normalized.contains("unavailable") || normalized.contains("expired") {
        return .red
    }
    return .secondary
}

private struct LinksConversationDetail: View {
    @ObservedObject var model: LinksMacOSAppModel
    @State private var repairConfirmationPresented = false
    @State private var removeConnectionConfirmationPresented = false

    var body: some View {
        if let conversation = model.selectedConversation {
            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    ProfileAvatar(title: conversation.title, size: 38)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(conversation.title)
                            .font(.title2.weight(.semibold))
                        Text("Private one-to-one conversation")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 7) {
                        HStack(spacing: 7) {
                            if model.canInitializeSelectedConversation {
                                Button {
                                    model.initializeSelectedConversation()
                                } label: {
                                    Label("Start secure chat", systemImage: "lock.badge.plus")
                                }
                                .buttonStyle(.borderedProminent)
                                .controlSize(.small)
                            }
                            Button {
                                repairConfirmationPresented = true
                            } label: {
                                Image(systemName: "arrow.triangle.2.circlepath")
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                            .disabled(!model.canInitializeSelectedConversation)
                            .help("Repair secure chat")
                            Button {
                                removeConnectionConfirmationPresented = true
                            } label: {
                                Image(systemName: "trash")
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                            .help("Remove this local connection")
                        }
                        StatusPill(title: model.connectionStatus,
                                   color: linksStatusColor(model.connectionStatus))
                    }
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 14)

                DeliveryStatusBanner(model: model)

                HStack(spacing: 7) {
                    Image(systemName: "lock.fill")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Text(model.conversationSetupStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Spacer()
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 8)

                Divider()
                MessageList(messages: conversation.messages)
                Divider()
                ComposerView(model: model)
            }
            .background(Color.primary.opacity(0.015))
            .alert("Repair secure chat?", isPresented: $repairConfirmationPresented) {
                Button("Repair", role: .destructive) {
                    model.resetSelectedConversation()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This replaces the broken encryption group. Messages already stuck in the old group cannot be recovered and will be discarded from delivery.")
            }
            .confirmationDialog(
                "Remove connection with \(conversation.title)?",
                isPresented: $removeConnectionConfirmationPresented,
                titleVisibility: .visible) {
                Button("Remove connection", role: .destructive) {
                    model.removeSelectedConnection()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This removes the local conversation only. Your account, contact, and identity stay safe.")
            }
            .task(id: conversation.id) {
                model.autoInitializeSelectedConversation()
            }
        } else {
            VStack(spacing: 14) {
                Image(systemName: "bubble.left.and.bubble.right")
                    .font(.system(size: 44))
                    .foregroundStyle(.secondary)
                Text("Choose a conversation")
                    .font(.title2.weight(.semibold))
                Text("Create a conversation to start encrypted messaging.")
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

private struct StatusPill: View {
    let title: String
    let color: Color

    var body: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
            Text(title)
                .font(.caption.weight(.medium))
                .foregroundStyle(.secondary)
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 5)
        .background(Color.primary.opacity(0.06))
        .clipShape(Capsule())
    }
}

private struct DeliveryStatusBanner: View {
    @ObservedObject var model: LinksMacOSAppModel

    private var state: LinksMacOSDeliveryState { model.deliveryState }

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: state.systemImage)
                .font(.callout.weight(.semibold))
                .foregroundStyle(color)
                .frame(width: 24, height: 24)
                .background(color.opacity(0.13))
                .clipShape(Circle())
            VStack(alignment: .leading, spacing: 2) {
                Text(state.title)
                    .font(.callout.weight(.semibold))
                Text(state.detail)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer(minLength: 8)
            if case .staleCursor = state {
                Button("Recover") {
                    model.recoverStaleCursor()
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
            }
        }
        .padding(.horizontal, 22)
        .padding(.vertical, 9)
        .background(color.opacity(0.07))
    }

    private var color: Color {
        switch state {
        case .ready: return .green
        case .connecting, .reconnecting, .offlineOutboxRetry: return .orange
        case .staleCursor, .authenticationExpired, .sendFailed, .dependencyOutage:
            return .red
        case .notConfigured, .offline: return .secondary
        }
    }
}

private struct MessageList: View {
    let messages: [LinksMacOSMessage]

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 14) {
                    if messages.isEmpty {
                        VStack(spacing: 8) {
                            Image(systemName: "lock.message")
                                .font(.system(size: 30))
                                .foregroundStyle(.tertiary)
                            Text("No messages yet")
                                .font(.headline)
                            Text("Messages in this conversation are end-to-end encrypted.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.top, 76)
                    } else {
                        ForEach(messages) { message in
                            MessageBubble(message: message)
                                .id(message.id)
                        }
                    }
                }
                .padding(24)
            }
            .onChange(of: messages.count) { _ in
                if let lastID = messages.last?.id {
                    withAnimation { proxy.scrollTo(lastID, anchor: .bottom) }
                }
            }
        }
    }
}

private struct MessageBubble: View {
    let message: LinksMacOSMessage

    var body: some View {
        HStack(alignment: .bottom) {
            if message.isOutgoing { Spacer(minLength: 90) }
            VStack(alignment: message.isOutgoing ? .trailing : .leading, spacing: 4) {
                Text(message.text)
                    .font(.body)
                    .textSelection(.enabled)
                    .padding(.horizontal, 13)
                    .padding(.vertical, 9)
                    .background(message.isOutgoing
                                ? Color.accentColor
                                : Color.primary.opacity(0.08))
                    .foregroundStyle(message.isOutgoing ? Color.white : Color.primary)
                    .clipShape(RoundedRectangle(cornerRadius: 13))
                Text(message.sentAt, style: .time)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: 520, alignment: message.isOutgoing ? .trailing : .leading)
            if !message.isOutgoing { Spacer(minLength: 90) }
        }
        .frame(maxWidth: .infinity)
    }
}

private struct ComposerView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Message", text: $model.composerText, axis: .vertical)
                .textFieldStyle(.plain)
                .lineLimit(1...5)
                .onSubmit { model.sendMessage() }
                .disabled(!model.canSendSelectedConversation)
                .padding(.horizontal, 12)
                .padding(.vertical, 9)
                .background(Color.primary.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 10))
            Button {
                model.sendMessage()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.title2.weight(.semibold))
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.large)
            .keyboardShortcut(.return, modifiers: [.command])
            .help("Send message")
            .disabled(!model.canSendSelectedConversation)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(.bar)
    }
}

private struct NewConversationView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var title = ""
    @State private var recipientUserID = ""
    @State private var validationError: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New conversation")
                .font(.title2.weight(.semibold))
            TextField("Name", text: $title)
                .textFieldStyle(.roundedBorder)
            TextField("Recipient user ID (UUID)", text: $recipientUserID)
                .textFieldStyle(.roundedBorder)
            Text("The recipient ID is public routing metadata. Message text stays inside the encrypted core.")
                .font(.caption)
                .foregroundStyle(.secondary)
            if let validationError {
                Text(validationError)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") {
                    if model.createConversation(title: title, recipientUserID: recipientUserID) {
                        dismiss()
                    } else {
                        validationError = "Enter a name and canonical recipient UUID."
                    }
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .padding(24)
        .frame(width: 430)
    }
}

private struct AddContactView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var handle = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Add contact")
                .font(.title2.weight(.semibold))
            Text("Find someone by their username. Only public handle, account ID, and active-device count are saved in this profile.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Username, for example alice", text: $handle)
                .textFieldStyle(.roundedBorder)
                .textContentType(.username)
            Text(model.contactStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Find and add") {
                    model.addContact(handle: handle)
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.isAddingContact)
            }
        }
        .padding(24)
        .frame(width: 460)
    }
}

private struct PairingView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("Approve device")
                .font(.title2.weight(.semibold))
            Text("Paste a signed links://connect link from the device joining this account. Review it before approval.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextEditor(text: $model.pairingInput)
                .font(.system(.body, design: .monospaced))
                .frame(minHeight: 110)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
            Text(model.pairingStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Approve device") {
                    model.approvePairing()
                }
                .buttonStyle(.borderedProminent)
                .disabled(model.isPairing)
            }
        }
        .padding(24)
        .frame(width: 520)
    }
}
