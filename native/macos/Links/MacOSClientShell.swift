import SwiftUI
import LinksClient

struct LinksRootView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        Group {
            if model.requiresOnboarding {
                LinksOnboardingView(model: model)
            } else if model.requiresAccountAuthentication {
                LinksAccountOnboardingView(model: model)
            } else {
                LinksMessagingView(model: model)
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
    @State private var showingPairing = false

    var body: some View {
        NavigationSplitView {
            LinksSidebar(model: model,
                         showingNewConversation: $showingNewConversation,
                         showingPairing: $showingPairing)
        } detail: {
            LinksConversationDetail(model: model)
        }
        .sheet(isPresented: $showingNewConversation) {
            NewConversationView(model: model)
        }
        .sheet(isPresented: $showingPairing) {
            PairingView(model: model)
        }
    }
}

private struct LinksSidebar: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var showingNewConversation: Bool
    @Binding var showingPairing: Bool

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Label("Links", systemImage: "lock.shield")
                    .font(.title2.weight(.semibold))
                Spacer()
                Button {
                    showingNewConversation = true
                } label: {
                    Image(systemName: "square.and.pencil")
                }
                .help("New conversation")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 14)

            List(selection: $model.selectedConversationID) {
                Section("Conversations") {
                    if model.conversations.isEmpty {
                        Text("No conversations yet")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(model.conversations) { conversation in
                            ConversationRow(conversation: conversation)
                                .tag(conversation.id as String?)
                        }
                    }
                }
            }
            .listStyle(.sidebar)

            Divider()
            VStack(alignment: .leading, spacing: 10) {
                StateRow(title: "Profile", value: model.profileName)
                StateRow(title: "Account", value: model.accountStatus)
                StateRow(title: "Device", value: model.deviceStatus)
                StateRow(title: "Connection", value: model.connectionStatus)
                StateRow(title: "Lifecycle", value: model.lifecycleStatus)
                HStack {
                    Text(model.packageStatus)
                    Spacer()
                    Button("Pair device") {
                        showingPairing = true
                    }
                    .buttonStyle(.link)
                    Button(model.hasMessagingHost ? "Disconnect" : "Connect") {
                        if model.hasMessagingHost {
                            model.disconnect()
                        } else {
                            model.connect()
                        }
                    }
                    .buttonStyle(.link)
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            .padding(16)
        }
        .frame(minWidth: 270)
    }
}

private struct ConversationRow: View {
    let conversation: LinksMacOSConversation

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "person.crop.circle")
                .font(.title3)
                .foregroundStyle(.secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text(conversation.title)
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
                .lineLimit(2)
        }
        .font(.caption)
    }
}

private struct LinksConversationDetail: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        if let conversation = model.selectedConversation {
            VStack(spacing: 0) {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(conversation.title)
                            .font(.title3.weight(.semibold))
                        Text("Encrypted one-to-one conversation")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Text(model.connectionStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 24)
                .padding(.vertical, 16)

                Divider()
                MessageList(messages: conversation.messages)
                Divider()
                ComposerView(model: model)
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

private struct MessageList: View {
    let messages: [LinksMacOSMessage]

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 10) {
                    if messages.isEmpty {
                        Text("No messages")
                            .foregroundStyle(.secondary)
                            .padding(.top, 32)
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
        HStack {
            if message.isOutgoing { Spacer(minLength: 90) }
            VStack(alignment: message.isOutgoing ? .trailing : .leading, spacing: 4) {
                Text(message.text)
                    .textSelection(.enabled)
                    .padding(.horizontal, 13)
                    .padding(.vertical, 9)
                    .background(message.isOutgoing ? Color.accentColor : Color.secondary.opacity(0.16))
                    .foregroundStyle(message.isOutgoing ? Color.white : Color.primary)
                    .clipShape(RoundedRectangle(cornerRadius: 13))
                Text(message.sentAt, style: .time)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }
            if !message.isOutgoing { Spacer(minLength: 90) }
        }
    }
}

private struct ComposerView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
            TextField("Message", text: $model.composerText, axis: .vertical)
                .textFieldStyle(.roundedBorder)
                .lineLimit(1...5)
                .onSubmit { model.sendMessage() }
            Button {
                model.sendMessage()
            } label: {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.title2)
            }
            .buttonStyle(.borderless)
            .keyboardShortcut(.return, modifiers: [.command])
            .help("Send message")
        }
        .padding(16)
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
