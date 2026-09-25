import PhotosUI
import SwiftUI
import LinksClient

struct IOSMobileRootView: View {
    @ObservedObject var model: IOSMobileAppModel

    var body: some View {
        ZStack {
            Color(uiColor: .systemGroupedBackground)
                .ignoresSafeArea()

            Group {
                if model.isAuthenticated {
                    IOSAuthenticatedShell(model: model)
                        .transition(.opacity.combined(with: .scale(scale: 0.98)))
                } else if model.requiresManualSignIn {
                    IOSOnboardingView(model: model)
                        .transition(.opacity)
                } else {
                    IOSSessionRestoreView(model: model)
                        .transition(.opacity)
                }
            }
        }
        .animation(.easeOut(duration: 0.25), value: model.isAuthenticated)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let error = model.error {
                IOSNoticeBanner(text: error, tint: .red) {
                    model.clearError()
                }
            }
        }
    }
}

private struct IOSSessionRestoreView: View {
    @ObservedObject var model: IOSMobileAppModel

    var body: some View {
        ZStack {
            Color(uiColor: .systemGroupedBackground)
                .ignoresSafeArea()

            VStack(spacing: 18) {
                Image(systemName: model.isRestoringSession
                      ? "arrow.triangle.2.circlepath.circle.fill"
                      : "exclamationmark.circle.fill")
                    .font(.system(size: 54))
                    .foregroundStyle(model.isRestoringSession
                                     ? IOSLinksPalette.cobalt : IOSLinksPalette.coral)

                Text(model.isRestoringSession
                     ? "Restoring your account"
                     : "Could not restore your account")
                    .font(.title2.weight(.bold))

                Text(model.isRestoringSession
                     ? "Your saved identity is signing in securely."
                     : "Try again, or log out to enter another account.")
                    .font(.subheadline)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: 300)

                if model.isRestoringSession {
                    ProgressView()
                        .controlSize(.large)
                } else {
                    Button("Try again") {
                        model.restoreSavedSession()
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(IOSLinksPalette.cobalt)

                    Button("Log out", role: .destructive) {
                        model.signOut()
                    }
                    .buttonStyle(.bordered)
                }
            }
            .padding(28)
        }
    }
}

private enum IOSLinksPalette {
    static let cobalt = Color(red: 0.25, green: 0.36, blue: 0.96)
    static let violet = Color(red: 0.48, green: 0.30, blue: 0.94)
    static let mint = Color(red: 0.16, green: 0.70, blue: 0.55)
    static let coral = Color(red: 0.96, green: 0.40, blue: 0.43)
    static let sky = Color(red: 0.20, green: 0.64, blue: 0.93)

    static let identityGradient = LinearGradient(
        colors: [cobalt, violet],
        startPoint: .topLeading,
        endPoint: .bottomTrailing)
}

private struct IOSOnboardingView: View {
    @ObservedObject var model: IOSMobileAppModel
    @FocusState private var usernameFocused: Bool
    @State private var showingPhoneLogin = false
    @State private var showingNewUsernameConfirmation = false

    var body: some View {
        NavigationStack {
            ZStack {
                Color(uiColor: .systemGroupedBackground)
                    .ignoresSafeArea()
                Circle()
                    .fill(IOSLinksPalette.cobalt.opacity(0.16))
                    .frame(width: 340, height: 340)
                    .blur(radius: 70)
                    .offset(x: 170, y: -310)
                    .allowsHitTesting(false)

                ScrollView {
                    VStack(spacing: 26) {
                        VStack(spacing: 14) {
                            ZStack {
                                RoundedRectangle(cornerRadius: 24, style: .continuous)
                                    .fill(IOSLinksPalette.identityGradient)
                                Image(systemName: "link.badge.plus")
                                    .font(.system(size: 36, weight: .semibold))
                                    .foregroundStyle(.white)
                            }
                            .frame(width: 78, height: 78)
                            .shadow(color: IOSLinksPalette.cobalt.opacity(0.28), radius: 22, y: 12)

                            Text("Private conversations,\nwithout the noise.")
                                .font(.system(size: 34, weight: .bold, design: .rounded))
                                .multilineTextAlignment(.center)
                                .foregroundStyle(.primary)

                            Text("Your identity stays on this device. Choose a username to continue.")
                                .font(.body)
                                .multilineTextAlignment(.center)
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: 330)
                        }
                        .padding(.top, 30)

                        VStack(spacing: 18) {
                            HStack(spacing: 5) {
                                ForEach(IOSUsernameAction.allCases) { action in
                                    Button {
                                        withAnimation(.easeOut(duration: 0.18)) {
                                            model.usernameAction = action
                                        }
                                    } label: {
                                        Text(action == .register ? "Create account" : "Log in")
                                            .font(.subheadline.weight(.semibold))
                                            .frame(maxWidth: .infinity)
                                            .padding(.vertical, 10)
                                    }
                                    .buttonStyle(.plain)
                                    .foregroundStyle(model.usernameAction == action ? .white : .primary)
                                    .background {
                                        RoundedRectangle(cornerRadius: 11, style: .continuous)
                                            .fill(model.usernameAction == action
                                                  ? IOSLinksPalette.cobalt
                                                  : Color.clear)
                                    }
                                }
                            }
                            .padding(4)
                            .background(Color.primary.opacity(0.055))
                            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))

                            HStack(spacing: 12) {
                                Image(systemName: "at")
                                    .font(.headline)
                                    .foregroundStyle(IOSLinksPalette.cobalt)
                                TextField("username", text: $model.username)
                                    .textInputAutocapitalization(.never)
                                    .autocorrectionDisabled()
                                    .textContentType(.username)
                                    .submitLabel(.continue)
                                    .focused($usernameFocused)
                                    .onSubmit { model.authenticateUsername() }
                            }
                            .padding(.horizontal, 15)
                            .frame(height: 54)
                            .background(Color(uiColor: .secondarySystemGroupedBackground))
                            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                            .overlay {
                                RoundedRectangle(cornerRadius: 16, style: .continuous)
                                    .stroke(usernameFocused
                                            ? IOSLinksPalette.cobalt
                                            : Color.primary.opacity(0.09),
                                            lineWidth: usernameFocused ? 2 : 1)
                            }

                            Button {
                                model.authenticateUsername()
                            } label: {
                                HStack(spacing: 9) {
                                    if model.isBusy {
                                        ProgressView()
                                            .tint(.white)
                                    } else {
                                        Image(systemName: model.usernameAction == .register
                                              ? "person.badge.plus" : "arrow.right")
                                    }
                                    Text(model.usernameAction == .register
                                         ? "Create my account" : "Continue to Links")
                                }
                                .font(.headline)
                                .foregroundStyle(.white)
                                .frame(maxWidth: .infinity)
                                .frame(height: 54)
                                .background(IOSLinksPalette.identityGradient)
                                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                            }
                            .buttonStyle(.plain)
                            .disabled(model.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                      || model.isBusy)
                            .opacity(model.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                     ? 0.55 : 1)

                            if model.isEnrolled {
                                Button {
                                    if model.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                                        usernameFocused = true
                                        model.error = "Enter a username first."
                                    } else {
                                        usernameFocused = false
                                        showingNewUsernameConfirmation = true
                                    }
                                } label: {
                                    Label("Enroll a new username", systemImage: "person.badge.plus")
                                        .font(.subheadline.weight(.semibold))
                                        .frame(maxWidth: .infinity)
                                        .frame(height: 46)
                                }
                                .buttonStyle(.borderedProminent)
                                .tint(IOSLinksPalette.violet)
                                .disabled(model.isBusy)
                            }

                            HStack(spacing: 7) {
                                Image(systemName: "lock.fill")
                                    .foregroundStyle(IOSLinksPalette.mint)
                                Text(model.isEnrolled
                                     ? "Protected by your hardware identity"
                                     : "A hardware identity will be created securely")
                            }
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                        }
                        .padding(20)
                        .background(.regularMaterial)
                        .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
                        .overlay {
                            RoundedRectangle(cornerRadius: 24, style: .continuous)
                                .stroke(Color.primary.opacity(0.07))
                        }

                        DisclosureGroup(isExpanded: $showingPhoneLogin) {
                            IOSPhoneLoginPanel(model: model)
                                .padding(.top, 14)
                        } label: {
                            Label("Use a phone number instead", systemImage: "iphone")
                                .font(.subheadline.weight(.semibold))
                        }
                        .tint(IOSLinksPalette.cobalt)
                        .padding(.horizontal, 4)

                        Text(model.status)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(.horizontal, 20)
                    .padding(.bottom, 36)
                }
                .scrollDismissesKeyboard(.interactively)
            }
            .toolbar(.hidden, for: .navigationBar)
            .onAppear { usernameFocused = true }
            .confirmationDialog("Enroll a new username?", isPresented: $showingNewUsernameConfirmation,
                                titleVisibility: .visible) {
                Button("Create new identity") { model.enrollNewUsername() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Your existing account stays on this device. A separate secure profile will be created for this username.")
            }
        }
    }
}

private struct IOSPhoneLoginPanel: View {
    @ObservedObject var model: IOSMobileAppModel

    var body: some View {
        VStack(spacing: 12) {
            HStack(spacing: 10) {
                Image(systemName: "phone.fill")
                    .foregroundStyle(IOSLinksPalette.coral)
                TextField("Phone number", text: $model.phone)
                    .keyboardType(.phonePad)
                    .textContentType(.telephoneNumber)
            }
            .padding(.horizontal, 14)
            .frame(height: 50)
            .background(Color(uiColor: .secondarySystemGroupedBackground))
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))

            Picker("Channel", selection: $model.channel) {
                Text("SMS").tag(IOSOTPChannel.sms)
                Text("WhatsApp").tag(IOSOTPChannel.whatsapp)
            }
            .pickerStyle(.segmented)

            Button("Send verification code") {
                model.sendVerificationCode()
            }
            .buttonStyle(.bordered)
            .disabled(!model.isEnrolled || model.isBusy)

            HStack(spacing: 10) {
                SecureField("Verification code", text: $model.verificationCode)
                    .keyboardType(.numberPad)
                    .textContentType(.oneTimeCode)
                Button("Verify") { model.verifyCode() }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.verificationCode.isEmpty || model.isBusy)
            }
        }
    }
}

private struct IOSAuthenticatedShell: View {
    @ObservedObject var model: IOSMobileAppModel

    var body: some View {
        TabView {
            IOSChatsView(model: model)
                .tabItem { Label("Chats", systemImage: "bubble.left.and.bubble.right.fill") }
                .badge(model.unreadConversationCount)
            IOSPeopleView(model: model)
                .tabItem { Label("People", systemImage: "person.2.fill") }
            IOSProfileView(model: model)
                .tabItem { Label("You", systemImage: "person.crop.circle.fill") }
        }
        .tint(IOSLinksPalette.cobalt)
        .background(Color(uiColor: .systemGroupedBackground).ignoresSafeArea())
        .toolbarBackground(Color(uiColor: .secondarySystemGroupedBackground), for: .tabBar)
        .toolbarBackground(.visible, for: .tabBar)
    }
}

private struct IOSChatsView: View {
    @ObservedObject var model: IOSMobileAppModel
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @State private var showingNewConversation = false
    @State private var showingNewGroup = false
    @State private var path = [String]()
    @State private var selectedConversationID: String?

    var body: some View {
        if horizontalSizeClass == .regular {
            padChats
        } else {
            phoneChats
        }
    }

    private var phoneChats: some View {
        NavigationStack(path: $path) {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    if model.conversations.isEmpty {
                        IOSEmptyConversations {
                            model.clearConversationCreationStatus()
                            showingNewConversation = true
                        }
                    } else {
                        LazyVStack(spacing: 10) {
                            ForEach(model.conversations) { conversation in
                                NavigationLink(value: conversation.id) {
                                    IOSConversationRow(
                                        conversation: conversation,
                                        imageJPEG: model.contactPictures[conversation.recipientUserID])
                                }
                                .buttonStyle(.plain)
                                .contextMenu {
                                    Button("Delete conversation", role: .destructive) {
                                        model.deleteConversation(conversation)
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 28)
            }
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle("Chats")
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    HStack(spacing: 6) {
                        Circle()
                            .fill(IOSLinksPalette.mint)
                            .frame(width: 8, height: 8)
                        Text("Secure")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(IOSLinksPalette.mint)
                    }
                    .padding(.horizontal, 9)
                    .padding(.vertical, 6)
                    .background(IOSLinksPalette.mint.opacity(0.11))
                    .clipShape(Capsule())

                    Button {
                        model.clearGroupStatus()
                        showingNewGroup = true
                    } label: {
                        Image(systemName: "person.3")
                    }
                    .accessibilityLabel("New group")

                    Button {
                        model.clearConversationCreationStatus()
                        showingNewConversation = true
                    } label: {
                        Image(systemName: "square.and.pencil")
                    }
                    .accessibilityLabel("New conversation")
                }
            }
            .sheet(isPresented: $showingNewGroup) {
                IOSNewGroupSheet(model: model) { conversation in
                    path.append(conversation.id)
                }
            }
            .navigationDestination(for: String.self) { conversationID in
                if let conversation = model.conversations.first(where: { $0.id == conversationID }) {
                    IOSConversationView(model: model, conversationID: conversation.id)
                } else {
                    Text("Conversation unavailable")
                        .foregroundStyle(.secondary)
                }
            }
            .onChange(of: model.conversations.map(\.id)) { ids in
                path.removeAll { !ids.contains($0) }
            }
            .sheet(isPresented: $showingNewConversation) {
                IOSNewConversationSheet(model: model) { conversation in
                    path.append(conversation.id)
                }
            }
        }
    }

    private var padChats: some View {
        NavigationSplitView {
            VStack(spacing: 0) {
                padChatsHeader

                Group {
                    if model.conversations.isEmpty {
                        IOSEmptyConversations {
                            model.clearConversationCreationStatus()
                            showingNewConversation = true
                        }
                    } else {
                        List(selection: $selectedConversationID) {
                            ForEach(model.conversations) { conversation in
                                IOSConversationRow(
                                    conversation: conversation,
                                    imageJPEG: model.contactPictures[conversation.recipientUserID],
                                    showsDisclosure: false)
                                    .tag(conversation.id)
                                    .contextMenu {
                                        Button("Delete conversation", role: .destructive) {
                                            model.deleteConversation(conversation)
                                        }
                                    }
                            }
                        }
                        .listStyle(.sidebar)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .toolbar(.hidden, for: .navigationBar)
        } detail: {
            IOSPadConversationDetail(model: model, conversationID: selectedConversationID)
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(isPresented: $showingNewGroup) {
            IOSNewGroupSheet(model: model) { conversation in
                selectedConversationID = conversation.id
            }
        }
        .sheet(isPresented: $showingNewConversation) {
            IOSNewConversationSheet(model: model) { conversation in
                selectedConversationID = conversation.id
            }
        }
        .onChange(of: model.conversations.map(\.id)) { ids in
            if let selectedConversationID, !ids.contains(selectedConversationID) {
                self.selectedConversationID = nil
            }
        }
    }

    private var padChatsHeader: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text("Chats")
                    .font(.largeTitle.weight(.bold))

                Spacer(minLength: 8)

                HStack(spacing: 6) {
                    Circle()
                        .fill(IOSLinksPalette.mint)
                        .frame(width: 8, height: 8)
                    Text("Secure")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(IOSLinksPalette.mint)
                }
                .padding(.horizontal, 9)
                .padding(.vertical, 6)
                .background(IOSLinksPalette.mint.opacity(0.11))
                .clipShape(Capsule())
            }

            HStack(spacing: 8) {
                Button {
                    model.clearGroupStatus()
                    showingNewGroup = true
                } label: {
                    VStack(spacing: 5) {
                        Image(systemName: "person.3")
                            .font(.body.weight(.semibold))
                        Text("New group")
                            .font(.subheadline.weight(.semibold))
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, minHeight: 54)
                }
                .accessibilityLabel("New group")

                Button {
                    model.clearConversationCreationStatus()
                    showingNewConversation = true
                } label: {
                    VStack(spacing: 5) {
                        Image(systemName: "square.and.pencil")
                            .font(.body.weight(.semibold))
                        Text("New chat")
                            .font(.subheadline.weight(.semibold))
                            .multilineTextAlignment(.center)
                    }
                    .frame(maxWidth: .infinity, minHeight: 54)
                }
                .accessibilityLabel("New conversation")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
        }
        .padding(.horizontal, 16)
        .padding(.top, 12)
        .padding(.bottom, 10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(uiColor: .secondarySystemGroupedBackground))
    }
}

private struct IOSPadConversationDetail: View {
    @ObservedObject var model: IOSMobileAppModel
    let conversationID: String?

    var body: some View {
        if let conversationID,
           model.conversations.contains(where: { $0.id == conversationID }) {
            IOSConversationView(model: model, conversationID: conversationID)
        } else {
            VStack(spacing: 12) {
                Image(systemName: "bubble.left.and.bubble.right")
                    .font(.system(size: 42, weight: .medium))
                    .foregroundStyle(IOSLinksPalette.cobalt)
                Text("Select a chat")
                    .font(.title2.weight(.semibold))
                Text("Choose a conversation, or start a new one.")
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .background(Color(uiColor: .systemGroupedBackground))
        }
    }
}

private struct IOSEmptyConversations: View {
    let action: () -> Void

    var body: some View {
        VStack(spacing: 18) {
            ZStack {
                Circle()
                    .fill(IOSLinksPalette.identityGradient)
                    .frame(width: 96, height: 96)
                Image(systemName: "bubble.left.and.bubble.right.fill")
                    .font(.system(size: 36, weight: .medium))
                    .foregroundStyle(.white)
            }
            .shadow(color: IOSLinksPalette.cobalt.opacity(0.24), radius: 20, y: 10)

            VStack(spacing: 7) {
                Text("Start with someone you trust")
                    .font(.title2.weight(.bold))
                Text("Find them by username and create a private conversation.")
                    .font(.subheadline)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: 290)
            }

            Button(action: action) {
                Label("New conversation", systemImage: "person.badge.plus")
                    .font(.headline)
                    .foregroundStyle(.white)
                    .padding(.horizontal, 20)
                    .frame(height: 50)
                    .background(IOSLinksPalette.identityGradient)
                    .clipShape(Capsule())
            }
            .buttonStyle(.plain)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 54)
        .padding(.horizontal, 20)
        .background(Color(uiColor: .secondarySystemGroupedBackground))
        .clipShape(RoundedRectangle(cornerRadius: 28, style: .continuous))
    }
}

private struct IOSConversationRow: View {
    let conversation: IOSMobileConversation
    var imageJPEG: Data? = nil
    var showsDisclosure = true

    var body: some View {
        if showsDisclosure {
            phoneRow
        } else {
            padRow
        }
    }

    private var phoneRow: some View {
        HStack(spacing: 13) {
            IOSAvatar(name: conversation.handle, size: 52, imageJPEG: imageJPEG)
            VStack(alignment: .leading, spacing: 5) {
                Text(conversation.displayTitle)
                    .font(.headline)
                    .foregroundStyle(.primary)
                Label(conversation.isGroup ? "Encrypted group" : "Private conversation",
                      systemImage: conversation.isGroup ? "person.3.fill" : "lock.fill")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 7) {
                Text(conversation.createdAt, style: .date)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                if conversation.unreadCount > 0 {
                    unreadBadge
                }
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(14)
        .background(Color(uiColor: .secondarySystemGroupedBackground))
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .stroke(Color.primary.opacity(0.055))
        }
    }

    private var padRow: some View {
        HStack(alignment: .top, spacing: 12) {
            IOSAvatar(name: conversation.handle, size: 44, imageJPEG: imageJPEG)

            VStack(alignment: .leading, spacing: 5) {
                Text(conversation.displayTitle)
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                    .truncationMode(.tail)
                    .fixedSize(horizontal: false, vertical: true)

                HStack(alignment: .top, spacing: 5) {
                    Image(systemName: conversation.isGroup ? "person.3.fill" : "lock.fill")
                        .font(.caption)
                        .padding(.top, 1)
                    Text(conversation.isGroup ? "Encrypted group" : "Private conversation")
                        .font(.caption)
                        .fixedSize(horizontal: false, vertical: true)
                }
                    .foregroundStyle(.secondary)

                HStack(spacing: 6) {
                    Text(conversation.createdAt, format: .dateTime.month(.abbreviated).day())
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)

                    Spacer(minLength: 0)

                    if conversation.unreadCount > 0 {
                        unreadBadge
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(.vertical, 9)
        .contentShape(Rectangle())
    }

    private var unreadBadge: some View {
        Text(conversation.unreadCount > 99 ? "99+" : "\(conversation.unreadCount)")
            .font(.caption2.weight(.bold))
            .foregroundStyle(.white)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(IOSLinksPalette.cobalt)
            .clipShape(Capsule())
            .accessibilityLabel("\(conversation.unreadCount) unread")
    }
}

private struct IOSNewConversationSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    let onCreated: (IOSMobileConversation) -> Void
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focused: Bool
    @State private var handle = ""

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                ZStack {
                    Circle()
                        .fill(IOSLinksPalette.cobalt.opacity(0.12))
                    Image(systemName: "person.crop.circle.badge.plus")
                        .font(.system(size: 34, weight: .medium))
                        .foregroundStyle(IOSLinksPalette.cobalt)
                }
                .frame(width: 82, height: 82)

                VStack(spacing: 7) {
                    Text("Who do you want to message?")
                        .font(.title2.weight(.bold))
                    Text("Search by their Links username.")
                        .foregroundStyle(.secondary)
                }

                HStack(spacing: 10) {
                    Text("@")
                        .font(.title3.weight(.bold))
                        .foregroundStyle(IOSLinksPalette.cobalt)
                    TextField("username", text: $handle)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .textContentType(.username)
                        .submitLabel(.go)
                        .focused($focused)
                        .onSubmit { createConversation() }
                }
                .padding(.horizontal, 15)
                .frame(height: 54)
                .background(Color(uiColor: .secondarySystemGroupedBackground))
                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                .overlay {
                    RoundedRectangle(cornerRadius: 16, style: .continuous)
                        .stroke(focused ? IOSLinksPalette.cobalt : Color.primary.opacity(0.09),
                                lineWidth: focused ? 2 : 1)
                }

                if let status = model.conversationCreationStatus {
                    Text(status)
                        .font(.subheadline)
                        .multilineTextAlignment(.center)
                        .foregroundStyle(status.contains("Could not")
                                         || status.contains("No Links")
                                         || status.contains("Choose someone")
                                         ? IOSLinksPalette.coral : .secondary)
                }

                Button(action: createConversation) {
                    HStack(spacing: 9) {
                        if model.isCreatingConversation {
                            ProgressView().tint(.white)
                        } else {
                            Image(systemName: "bubble.left.and.bubble.right.fill")
                        }
                        Text("Create conversation")
                    }
                    .font(.headline)
                    .foregroundStyle(.white)
                    .frame(maxWidth: .infinity)
                    .frame(height: 52)
                    .background(IOSLinksPalette.identityGradient)
                    .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                }
                .buttonStyle(.plain)
                .disabled(handle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || model.isCreatingConversation)
                .opacity(handle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? 0.55 : 1)

                Spacer()
            }
            .padding(24)
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle("New chat")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium])
    }

    private func createConversation() {
        Task {
            if let conversation = await model.createConversation(handle: handle) {
                onCreated(conversation)
                dismiss()
            }
        }
    }
}

private struct IOSPeopleView: View {
    @ObservedObject var model: IOSMobileAppModel
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    @State private var showingNewConversation = false
    @State private var contactToRemove: IOSMobileContact?
    @State private var path = [String]()
    @State private var selectedConversationID: String?

    var body: some View {
        if horizontalSizeClass == .regular {
            padPeople
        } else {
            phonePeople
        }
    }

    private var phonePeople: some View {
        NavigationStack(path: $path) {
            Group {
                if model.contacts.isEmpty {
                    VStack(spacing: 15) {
                        Image(systemName: "person.2")
                            .font(.system(size: 44))
                            .foregroundStyle(IOSLinksPalette.violet)
                        Text("Your people will appear here")
                            .font(.title3.weight(.semibold))
                        Text("Add someone by username to keep them close.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                        Button("Add someone") {
                            model.clearConversationCreationStatus()
                            showingNewConversation = true
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color(uiColor: .systemGroupedBackground))
                } else {
                    List {
                        ForEach(model.contacts) { contact in
                            Button {
                                Task {
                                    if let conversation = await model.createConversation(handle: contact.handle) {
                                        path.append(conversation.id)
                                    }
                                }
                            } label: {
                                HStack(spacing: 13) {
                                    IOSAvatar(
                                        name: contact.handle,
                                        size: 46,
                                        imageJPEG: model.contactPictures[contact.userID])
                                    VStack(alignment: .leading, spacing: 4) {
                                        Text("@\(contact.handle)")
                                            .font(.headline)
                                            .foregroundStyle(.primary)
                                        Text(contact.deviceCount == 1
                                             ? "1 secure device" : "\(contact.deviceCount) secure devices")
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    Image(systemName: "bubble.left.fill")
                                        .foregroundStyle(IOSLinksPalette.cobalt)
                                }
                                .padding(.vertical, 5)
                            }
                            .buttonStyle(.plain)
                            .contextMenu {
                                Button("Remove Contact", role: .destructive) {
                                    contactToRemove = contact
                                }
                            }
                            .swipeActions {
                                Button("Remove", role: .destructive) {
                                    contactToRemove = contact
                                }
                            }
                        }
                    }
                    .listStyle(.insetGrouped)
                }
            }
            .navigationTitle("People")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.clearConversationCreationStatus()
                        showingNewConversation = true
                    } label: {
                        Image(systemName: "person.badge.plus")
                    }
                    .accessibilityLabel("Add someone")
                }
            }
            .navigationDestination(for: String.self) { conversationID in
                if let conversation = model.conversations.first(where: { $0.id == conversationID }) {
                    IOSConversationView(model: model, conversationID: conversation.id)
                } else {
                    Text("Conversation unavailable")
                        .foregroundStyle(.secondary)
                }
            }
            .sheet(isPresented: $showingNewConversation) {
                IOSNewConversationSheet(model: model) { conversation in
                    path.append(conversation.id)
                }
            }
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
        }
    }

    private var padPeople: some View {
        NavigationSplitView {
            Group {
                if model.contacts.isEmpty {
                    VStack(spacing: 15) {
                        Image(systemName: "person.2")
                            .font(.system(size: 44))
                            .foregroundStyle(IOSLinksPalette.violet)
                        Text("Your people will appear here")
                            .font(.title3.weight(.semibold))
                        Text("Add someone by username to keep them close.")
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                        Button("Add someone") {
                            model.clearConversationCreationStatus()
                            showingNewConversation = true
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .padding(24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    List(model.contacts) { contact in
                        Button {
                            Task {
                                if let conversation = await model.createConversation(handle: contact.handle) {
                                    selectedConversationID = conversation.id
                                }
                            }
                        } label: {
                            HStack(spacing: 13) {
                                IOSAvatar(
                                    name: contact.handle,
                                    size: 40,
                                    imageJPEG: model.contactPictures[contact.userID])
                                VStack(alignment: .leading, spacing: 3) {
                                    Text("@\(contact.handle)")
                                        .font(.headline)
                                        .foregroundStyle(.primary)
                                    Text(contact.deviceCount == 1
                                         ? "1 secure device" : "\(contact.deviceCount) secure devices")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .padding(.vertical, 4)
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button("Remove Contact", role: .destructive) {
                                contactToRemove = contact
                            }
                        }
                    }
                    .listStyle(.sidebar)
                }
            }
            .navigationTitle("People")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.clearConversationCreationStatus()
                        showingNewConversation = true
                    } label: {
                        Image(systemName: "person.badge.plus")
                    }
                    .accessibilityLabel("Add someone")
                }
            }
        } detail: {
            IOSPadConversationDetail(model: model, conversationID: selectedConversationID)
        }
        .navigationSplitViewStyle(.balanced)
        .sheet(isPresented: $showingNewConversation) {
            IOSNewConversationSheet(model: model) { conversation in
                selectedConversationID = conversation.id
            }
        }
        .alert("Remove contact?", isPresented: Binding(
            get: { contactToRemove != nil },
            set: { isPresented in
                if !isPresented { contactToRemove = nil }
            })) {
                Button("Remove", role: .destructive) {
                    if let contact = contactToRemove {
                        model.removeContact(contact)
                    }
                    contactToRemove = nil
                }
                Button("Cancel", role: .cancel) { contactToRemove = nil }
            } message: {
                Text("This removes the saved contact. Existing conversations and messages stay.")
            }
        .onChange(of: model.conversations.map(\.id)) { ids in
            if let selectedConversationID, !ids.contains(selectedConversationID) {
                self.selectedConversationID = nil
            }
        }
    }
}

private struct IOSProfileView: View {
    @ObservedObject var model: IOSMobileAppModel
    @State private var showingPairDevice = false
    @State private var showingSignOut = false
    @State private var showingNewUsername = false
    @State private var showingUsernameChange = false
    @State private var showingDisplayNameChange = false
    @State private var displayNameDraft = ""
    @State private var pictureItem: PhotosPickerItem?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 18) {
                    VStack(spacing: 12) {
                        IOSAvatar(name: model.profileDisplayName, size: 82,
                                  imageJPEG: model.profilePictureJPEG)
                        HStack(spacing: 10) {
                            PhotosPicker(selection: $pictureItem, matching: .images) {
                                Label(model.profilePictureJPEG == nil ? "Add picture" : "Change picture",
                                      systemImage: "photo")
                            }
                            .buttonStyle(.bordered)
                            if model.profilePictureJPEG != nil {
                                Button("Remove picture", role: .destructive) {
                                    model.removeProfilePicture()
                                }
                                .buttonStyle(.bordered)
                            }
                        }
                        .onChange(of: pictureItem) { item in
                            guard let item else { return }
                            Task {
                                if let data = try? await item.loadTransferable(type: Data.self) {
                                    model.replaceProfilePicture(with: data)
                                } else {
                                    model.error = "The picture could not be read."
                                }
                                pictureItem = nil
                            }
                        }
                        Text(model.profileDisplayName)
                            .font(.title2.weight(.bold))
                        Label("Identity protected on this device", systemImage: "checkmark.shield.fill")
                            .font(.subheadline.weight(.medium))
                            .foregroundStyle(IOSLinksPalette.mint)
                    }
                    .padding(.vertical, 22)

                    VStack(spacing: 0) {
                        Button {
                            displayNameDraft = model.profileDisplayName
                            model.profileDisplayNameError = nil
                            showingDisplayNameChange = true
                        } label: {
                            IOSProfileRow(icon: "person.crop.circle", color: IOSLinksPalette.cobalt,
                                          title: "Display name", value: model.profileDisplayName,
                                          showsDisclosure: true)
                        }
                        .buttonStyle(.plain)
                        .sheet(isPresented: $showingDisplayNameChange) {
                            IOSChangeDisplayNameSheet(model: model, displayName: $displayNameDraft)
                        }
                        Divider().padding(.leading, 54)
                        Button {
                            model.clearError()
                            showingUsernameChange = true
                        } label: {
                            IOSProfileRow(icon: "at", color: IOSLinksPalette.cobalt,
                                          title: "Username",
                                          value: model.accountUsername.map { "@\($0)" } ?? "Not set",
                                          showsDisclosure: true)
                        }
                        .buttonStyle(.plain)
                        Divider().padding(.leading, 54)
                        IOSProfileRow(icon: "person.text.rectangle", color: IOSLinksPalette.cobalt,
                                      title: "Account", value: model.accountStatus)
                        Divider().padding(.leading, 54)
                        IOSProfileRow(icon: "iphone", color: IOSLinksPalette.violet,
                                      title: "Device", value: model.deviceStatus)
                        Divider().padding(.leading, 54)
                        Button {
                            showingPairDevice = true
                        } label: {
                            IOSProfileRow(icon: "person.2.badge.plus", color: IOSLinksPalette.sky,
                                          title: "Link another device", value: "")
                        }
                        .buttonStyle(.plain)
                    }
                    .background(Color(uiColor: .secondarySystemGroupedBackground))
                    .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))

                    Button(role: .destructive) {
                        showingSignOut = true
                    } label: {
                        Label("Sign out", systemImage: "rectangle.portrait.and.arrow.right")
                            .font(.headline)
                            .frame(maxWidth: .infinity)
                            .frame(height: 50)
                    }
                    .buttonStyle(.bordered)
                    .tint(IOSLinksPalette.coral)

                    Button {
                        showingNewUsername = true
                    } label: {
                        Label("Enroll another username", systemImage: "person.badge.plus")
                            .font(.subheadline.weight(.semibold))
                            .frame(maxWidth: .infinity)
                            .frame(height: 46)
                    }
                    .buttonStyle(.bordered)
                    .tint(IOSLinksPalette.violet)
                }
                .padding(.horizontal, 18)
                .padding(.bottom, 28)
                .frame(maxWidth: 720)
                .frame(maxWidth: .infinity)
            }
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle("You")
            .sheet(isPresented: $showingPairDevice) {
                IOSPairDeviceSheet(model: model)
            }
            .sheet(isPresented: $showingUsernameChange) {
                IOSChangeUsernameSheet(model: model)
            }
            .confirmationDialog("Sign out of Links?", isPresented: $showingSignOut,
                                titleVisibility: .visible) {
                Button("Sign out", role: .destructive) { model.signOut() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Your hardware identity and saved conversations stay on this device.")
            }
            .confirmationDialog("Enroll another username?", isPresented: $showingNewUsername,
                                titleVisibility: .visible) {
                Button("Continue") { model.signOut() }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("Links will keep this account saved, then let you create a separate username.")
            }
        }
    }
}

private struct IOSChangeUsernameSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var handle = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("New username") {
                    TextField("username", text: $handle)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .textContentType(.username)
                } footer: {
                    Text("3–32 lowercase letters, numbers, or underscores. Your old username becomes available to others.")
                }

                if let error = model.error {
                    Text(error)
                        .foregroundStyle(.red)
                }
            }
            .navigationTitle("Change username")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        Task {
                            if await model.changeUsername(handle) {
                                dismiss()
                            }
                        }
                    } label: {
                        if model.isChangingUsername {
                            ProgressView()
                        } else {
                            Text("Save")
                        }
                    }
                    .disabled(handle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                              || model.isChangingUsername)
                }
            }
            .onAppear {
                handle = model.accountUsername ?? ""
                Task {
                    if let current = await model.refreshCurrentUsername() {
                        handle = current
                    }
                }
            }
        }
        .presentationDetents([.medium])
    }
}

private struct IOSChangeDisplayNameSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    @Binding var displayName: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            Form {
                Section("Display name") {
                    TextField("Name", text: $displayName)
                        .textInputAutocapitalization(.words)
                        .autocorrectionDisabled()
                } footer: {
                    Text("Shown in this client. Your username and saved profile stay the same.")
                }

                if let error = model.profileDisplayNameError {
                    Text(error)
                        .foregroundStyle(.red)
                }
            }
            .navigationTitle("Change display name")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        if model.changeProfileDisplayName(displayName) {
                            dismiss()
                        }
                    }
                    .disabled(displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
        }
        .presentationDetents([.medium])
    }
}

private struct IOSProfileRow: View {
    let icon: String
    let color: Color
    let title: String
    let value: String
    var showsDisclosure = false

    var body: some View {
        HStack(spacing: 13) {
            Image(systemName: icon)
                .font(.headline)
                .foregroundStyle(color)
                .frame(width: 34, height: 34)
                .background(color.opacity(0.12))
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            VStack(alignment: .leading, spacing: 3) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                if !value.isEmpty {
                    Text(value)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }
            Spacer()
            if value.isEmpty || showsDisclosure {
                Image(systemName: "chevron.right")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.tertiary)
            }
        }
        .padding(14)
        .contentShape(Rectangle())
    }
}

private struct IOSPairDeviceSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 18) {
                Label("Approve a trusted device", systemImage: "person.2.badge.plus")
                    .font(.title2.weight(.bold))
                    .foregroundStyle(IOSLinksPalette.violet)
                Text("Paste the signed Links pairing address shown on the other device.")
                    .foregroundStyle(.secondary)
                TextEditor(text: $model.pairingInput)
                    .font(.system(.footnote, design: .monospaced))
                    .padding(10)
                    .frame(minHeight: 130)
                    .background(Color(uiColor: .secondarySystemGroupedBackground))
                    .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                Text(model.pairingStatus)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Button {
                    model.approvePairing()
                } label: {
                    Label("Approve device", systemImage: "checkmark.shield.fill")
                        .font(.headline)
                        .foregroundStyle(.white)
                        .frame(maxWidth: .infinity)
                        .frame(height: 50)
                        .background(IOSLinksPalette.identityGradient)
                        .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                }
                .buttonStyle(.plain)
                .disabled(model.pairingInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || model.isBusy)
                Spacer()
            }
            .padding(20)
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle("Link device")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
            }
        }
        .presentationDetents([.medium, .large])
    }
}

private struct IOSConversationView: View {
    @ObservedObject var model: IOSMobileAppModel
    let conversationID: String
    @State private var composerText = ""
    @State private var showingMembers = false
    @FocusState private var composerFocused: Bool

    private let conversationBottomID = "conversation-bottom"

    private var conversation: IOSMobileConversation? {
        model.conversation(withID: conversationID)
    }

    private var setupStatusText: String {
        if let conversation, conversation.isGroup {
            return conversation.groupActive
                ? "End-to-end encrypted group"
                : "You are no longer a member of this group"
        }
        if conversation?.isSecureReady == true { return model.messagingStatus }
        if model.messagingState != .ready { return model.messagingStatus }
        if !model.preKeyStatus.hasPrefix("Ready") { return model.preKeyStatus }
        return model.preparingConversationIDs.contains(conversationID)
            ? "Establishing a secure conversation" : "Secure setup needs attention"
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 7) {
                if model.preparingConversationIDs.contains(conversationID) {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Image(systemName: conversation?.isSecureReady == true
                          && model.messagingState == .ready
                          ? "lock.fill" : "arrow.triangle.2.circlepath")
                }
                Text(setupStatusText)
            }
            .font(.caption.weight(.medium))
            .foregroundStyle(conversation?.isSecureReady == true
                             ? IOSLinksPalette.mint : IOSLinksPalette.violet)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .frame(maxWidth: .infinity)
            .background((conversation?.isSecureReady == true
                         ? IOSLinksPalette.mint : IOSLinksPalette.violet).opacity(0.09))

            if let conversation, conversation.messages.isEmpty {
                Spacer()
                VStack(spacing: 14) {
                    IOSAvatar(name: conversation.handle, size: 72)
                    Text(conversation.displayTitle)
                        .font(.title2.weight(.bold))
                    Text(conversation.isSecureReady
                         ? "Messages are protected with end-to-end encryption."
                         : setupStatusText + "…")
                        .font(.subheadline)
                        .multilineTextAlignment(.center)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: 310)
                }
                Spacer()
            } else if let conversation {
                ScrollViewReader { proxy in
                    ScrollView {
                        LazyVStack(spacing: 8) {
                            ForEach(conversation.messages) { message in
                                IOSMessageBubble(message: message,
                                                 senderLabel: model.senderLabel(for: message))
                                    .id(message.id)
                            }
                            Color.clear
                                .frame(height: 1)
                                .id(conversationBottomID)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 16)
                    }
                    .onAppear {
                        DispatchQueue.main.async {
                            proxy.scrollTo(conversationBottomID, anchor: .bottom)
                        }
                    }
                    .onChange(of: conversation.messages.count) { _ in
                        DispatchQueue.main.async {
                            withAnimation {
                                proxy.scrollTo(conversationBottomID, anchor: .bottom)
                            }
                        }
                    }
                    .onReceive(NotificationCenter.default.publisher(
                        for: UIResponder.keyboardDidShowNotification)) { _ in
                        DispatchQueue.main.async {
                            withAnimation {
                                proxy.scrollTo(conversationBottomID, anchor: .bottom)
                            }
                        }
                    }
                }
            }

            HStack(alignment: .bottom, spacing: 10) {
                TextField("Message", text: $composerText, axis: .vertical)
                    .lineLimit(1...5)
                    .focused($composerFocused)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    .background(Color(uiColor: .secondarySystemGroupedBackground))
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

                Button {
                    if model.sendMessage(conversationID: conversationID, text: composerText) {
                        composerText = ""
                    }
                } label: {
                    Image(systemName: "arrow.up")
                        .font(.headline.weight(.bold))
                        .foregroundStyle(.white)
                        .frame(width: 42, height: 42)
                        .background(IOSLinksPalette.identityGradient)
                        .clipShape(Circle())
                }
                .buttonStyle(.plain)
                .disabled(composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || conversation?.isSecureReady != true
                          || conversation?.groupActive == false)
                .opacity(composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                         || conversation?.isSecureReady != true
                         || conversation?.groupActive == false ? 0.45 : 1)
                .accessibilityLabel("Send message")
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(.regularMaterial)
        }
        .background(Color(uiColor: .systemGroupedBackground))
        .navigationTitle(conversation?.displayTitle ?? "Conversation")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if conversation?.isGroup == true {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        model.clearGroupStatus()
                        showingMembers = true
                    } label: {
                        Image(systemName: "person.3")
                    }
                    .accessibilityLabel("Group members")
                }
            }
        }
        .sheet(isPresented: $showingMembers) {
            IOSGroupMembersSheet(model: model, conversationID: conversationID)
        }
        .onAppear {
            model.markConversationRead(conversationID)
        }
        .onDisappear {
            model.markConversationClosed(conversationID)
        }
        .task(id: conversationID) {
            await model.prepareConversation(conversationID)
        }
    }
}

private struct IOSMessageBubble: View {
    let message: IOSMobileMessage
    var senderLabel: String? = nil

    var body: some View {
        HStack {
            if message.isOutgoing { Spacer(minLength: 52) }
            VStack(alignment: message.isOutgoing ? .trailing : .leading, spacing: 4) {
                if let senderLabel {
                    Text(senderLabel)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(IOSLinksPalette.cobalt)
                }
                Text(message.text)
                    .font(.body)
                    .foregroundStyle(message.isOutgoing ? .white : .primary)
                    .textSelection(.enabled)
                Text(message.sentAt, style: .time)
                    .font(.caption2)
                    .foregroundStyle(message.isOutgoing ? .white.opacity(0.72) : .secondary)
            }
            .padding(.horizontal, 13)
            .padding(.vertical, 9)
            .background(message.isOutgoing
                        ? AnyShapeStyle(IOSLinksPalette.identityGradient)
                        : AnyShapeStyle(Color(uiColor: .secondarySystemGroupedBackground)))
            .clipShape(RoundedRectangle(cornerRadius: 17, style: .continuous))
            if !message.isOutgoing { Spacer(minLength: 52) }
        }
    }
}

private struct IOSAvatar: View {
    let name: String
    let size: CGFloat
    var imageJPEG: Data? = nil

    private var initial: String {
        String(name.trimmingCharacters(in: CharacterSet(charactersIn: "@ ")).prefix(1)).uppercased()
    }

    private var accent: Color {
        let value = name.utf8.reduce(0) { ($0 + Int($1)) % 4 }
        return [IOSLinksPalette.cobalt, IOSLinksPalette.violet,
                IOSLinksPalette.sky, IOSLinksPalette.coral][value]
    }

    var body: some View {
        Group {
            if let imageJPEG, let image = UIImage(data: imageJPEG) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Text(initial)
                    .font(.system(size: size * 0.40, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
            }
        }
        .frame(width: size, height: size)
        .background(
            LinearGradient(colors: [accent, accent.opacity(0.72)],
                           startPoint: .topLeading, endPoint: .bottomTrailing))
        .clipShape(Circle())
        .overlay(Circle().stroke(.white.opacity(0.28), lineWidth: 1))
    }
}

private struct IOSNoticeBanner: View {
    let text: String
    let tint: Color
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill")
                .foregroundStyle(tint)
            Text(text)
                .font(.footnote)
                .frame(maxWidth: .infinity, alignment: .leading)
            Button("Dismiss", action: dismiss)
                .font(.footnote.weight(.semibold))
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 12)
        .background(.ultraThinMaterial)
        .overlay(alignment: .top) {
            Divider().overlay(tint.opacity(0.35))
        }
    }
}

private struct IOSContactPickerSection: View {
    let contacts: [IOSMobileContact]
    @Binding var selection: Set<String>

    var body: some View {
        if contacts.isEmpty {
            Text("Add people in the People tab first.")
                .foregroundStyle(.secondary)
        } else {
            ForEach(contacts) { contact in
                Button {
                    if selection.contains(contact.userID) {
                        selection.remove(contact.userID)
                    } else {
                        selection.insert(contact.userID)
                    }
                } label: {
                    HStack {
                        Text("@\(contact.handle)")
                            .foregroundStyle(.primary)
                        Spacer()
                        if selection.contains(contact.userID) {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(IOSLinksPalette.cobalt)
                        }
                    }
                }
            }
        }
    }
}

private struct IOSNewGroupSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    let onCreated: (IOSMobileConversation) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var selection = Set<String>()

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Group name", text: $name)
                } footer: {
                    Text("The server stores who is in the group, never its name or messages.")
                }
                Section("Invite") {
                    IOSContactPickerSection(contacts: model.contacts, selection: $selection)
                }
                if !model.groupStatus.isEmpty {
                    Section {
                        Text(model.groupStatus)
                            .foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle("New group")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    if model.isUpdatingGroup {
                        ProgressView()
                    } else {
                        Button("Create") {
                            Task {
                                if let conversation = await model.createGroup(
                                    name: name, memberUserIDs: Array(selection)) {
                                    dismiss()
                                    onCreated(conversation)
                                }
                            }
                        }
                        .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                  || selection.isEmpty)
                    }
                }
            }
        }
    }
}

private struct IOSGroupMembersSheet: View {
    @ObservedObject var model: IOSMobileAppModel
    let conversationID: String
    @Environment(\.dismiss) private var dismiss
    @State private var selection = Set<String>()
    @State private var leaveConfirmationPresented = false
    @State private var disbandConfirmationPresented = false
    @State private var memberToOwn: IOSMobileGroupMember?

    private var members: [IOSMobileGroupMember] { model.groupMembers[conversationID] ?? [] }
    private var myRole: IOSUsernameAuthClient.GroupRole? { model.role(in: conversationID) }

    private var invitableContacts: [IOSMobileContact] {
        let memberIDs = Set(members.map(\.userID))
        return model.contacts.filter { !memberIDs.contains($0.userID) }
    }

    private func canRemove(_ member: IOSMobileGroupMember) -> Bool {
        guard model.canManageGroup(conversationID), !member.isSelf, member.role != .owner else {
            return false
        }
        return myRole == .owner || member.role != .admin
    }

    var body: some View {
        NavigationStack {
            List {
                Section("Members") {
                    ForEach(members) { member in
                        HStack {
                            Text(member.isSelf ? "\(member.displayName) (you)" : member.displayName)
                            if let role = member.role, role != .member {
                                Text(role == .owner ? "Owner" : "Admin")
                                    .font(.caption2.weight(.semibold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(IOSLinksPalette.cobalt.opacity(0.14))
                                    .clipShape(Capsule())
                            }
                        }
                        .swipeActions {
                            if canRemove(member) {
                                Button("Remove", role: .destructive) {
                                    Task { await model.removeMember(member.userID, from: conversationID) }
                                }
                            }
                            if myRole == .owner, !member.isSelf, member.role != .owner {
                                Button("Make owner") {
                                    memberToOwn = member
                                }
                                .tint(IOSLinksPalette.cobalt)
                            }
                            if myRole == .owner, !member.isSelf, member.role == .member {
                                Button("Make admin") {
                                    Task { await model.makeAdmin(member.userID, in: conversationID) }
                                }
                                .tint(IOSLinksPalette.cobalt)
                            }
                        }
                    }
                }
                if model.canManageGroup(conversationID) {
                    Section("Add people") {
                        IOSContactPickerSection(contacts: invitableContacts, selection: $selection)
                        if !selection.isEmpty {
                            Button("Add \(selection.count == 1 ? "1 person" : "\(selection.count) people")") {
                                let chosen = Array(selection)
                                selection.removeAll()
                                Task { await model.addMembers(chosen, to: conversationID) }
                            }
                            .disabled(model.isUpdatingGroup)
                        }
                    }
                }
                if !model.groupStatus.isEmpty {
                    Section {
                        Text(model.groupStatus)
                            .foregroundStyle(.secondary)
                    }
                }
                if model.conversation(withID: conversationID)?.groupActive == true {
                    Section {
                        if myRole == .owner {
                            Button("Disband group", role: .destructive) {
                                disbandConfirmationPresented = true
                            }
                        } else {
                            Button("Leave group", role: .destructive) {
                                leaveConfirmationPresented = true
                            }
                        }
                    }
                }
            }
            .navigationTitle(model.conversation(withID: conversationID)?.handle ?? "Group")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { dismiss() }
                }
            }
            .task { await model.refreshGroupMembers(conversationID) }
            .confirmationDialog(
                "Make \(memberToOwn?.displayName ?? "this member") the owner?",
                isPresented: Binding(get: { memberToOwn != nil },
                                     set: { if !$0 { memberToOwn = nil } }),
                titleVisibility: .visible
            ) {
                Button("Make owner") {
                    if let member = memberToOwn {
                        Task { await model.giveOwnership(to: member.userID, in: conversationID) }
                    }
                    memberToOwn = nil
                }
            } message: {
                Text("You become a member. The owner can disband the group, and you can leave it.")
            }
            .confirmationDialog("Disband this group?", isPresented: $disbandConfirmationPresented,
                                titleVisibility: .visible) {
                Button("Disband", role: .destructive) {
                    Task {
                        await model.disbandGroup(conversationID)
                        if model.conversation(withID: conversationID) == nil {
                            dismiss()
                        }
                    }
                }
            } message: {
                Text("Everyone is removed and the group is deleted. Messages already on their devices stay there.")
            }
            .confirmationDialog("Leave this group?", isPresented: $leaveConfirmationPresented,
                                titleVisibility: .visible) {
                Button("Leave", role: .destructive) {
                    Task {
                        await model.leaveGroup(conversationID)
                        dismiss()
                    }
                }
            } message: {
                Text("You leave the group and it disappears from your chats.")
            }
        }
    }
}
