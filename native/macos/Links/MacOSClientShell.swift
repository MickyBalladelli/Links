import SwiftUI

struct LinksRootView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        Group {
            if model.requiresOnboarding {
                LinksOnboardingView(model: model)
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
            Text("Account sign-in and encrypted transport appear after local identity setup.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(48)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct LinksMessagingView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @State private var showingNewConversation = false

    var body: some View {
        NavigationSplitView {
            LinksSidebar(model: model, showingNewConversation: $showingNewConversation)
        } detail: {
            LinksConversationDetail(model: model)
        }
        .sheet(isPresented: $showingNewConversation) {
            NewConversationView(model: model)
        }
    }
}

private struct LinksSidebar: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var showingNewConversation: Bool

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
