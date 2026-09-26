import AppKit
import SwiftUI
import UniformTypeIdentifiers
import LinksClient

struct LinksRootView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(spacing: 0) {
            Group {
                if model.requiresOnboarding {
                    LinksOnboardingView(model: model)
                } else if model.shouldRestoreSavedSession || model.isRestoringSession {
                    LinksSessionRestoreView(model: model)
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

private struct LinksSessionRestoreView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: model.isRestoringSession
                  ? "arrow.triangle.2.circlepath.circle.fill"
                  : "exclamationmark.circle.fill")
                .font(.system(size: 48))
                .foregroundStyle(model.isRestoringSession ? .blue : .orange)

            Text(model.isRestoringSession
                 ? "Restoring your account"
                 : "Could not restore your account")
                .font(.title2.weight(.semibold))

            Text(model.isRestoringSession
                 ? "Your saved Mac identity is signing in securely."
                 : "Try again, or log out to enter another account.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 360)

            if model.isRestoringSession {
                ProgressView()
                    .controlSize(.small)
            } else {
                HStack(spacing: 10) {
                    Button("Try again") {
                        model.restoreSavedSession()
                    }
                    .buttonStyle(.borderedProminent)

                    Button("Log out", role: .destructive) {
                        model.logout()
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(48)
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
    @FocusState private var usernameFieldFocused: Bool
    @State private var showingPhoneAccount = false
    @State private var showingPairing = false

    var body: some View {
        ZStack {
            AccountOnboardingBackground()

            ScrollView {
                VStack(spacing: 24) {
                    AccountOnboardingHero()

                    VStack(alignment: .leading, spacing: 0) {
                        VStack(alignment: .leading, spacing: 18) {
                            HStack(spacing: 10) {
                                IconBadge(systemImage: "person.crop.circle.badge.checkmark",
                                          tint: .blue)
                                VStack(alignment: .leading, spacing: 3) {
                                    Text("Choose how to continue")
                                        .font(.title3.weight(.semibold))
                                    Text("A username is the fastest way to get started.")
                                        .font(.callout)
                                        .foregroundStyle(.secondary)
                                }
                            }

                            HStack(spacing: 8) {
                                ForEach(LinksMacOSAuthMode.allCases) { mode in
                                    AccountModeButton(
                                        mode: mode,
                                        isSelected: model.authMode == mode) {
                                            model.authMode = mode
                                        }
                                }
                            }
                            .padding(5)
                            .background(Color.primary.opacity(0.06))
                            .clipShape(RoundedRectangle(cornerRadius: 12))

                            VStack(alignment: .leading, spacing: 8) {
                                Text("Username")
                                    .font(.callout.weight(.semibold))
                                HStack(spacing: 10) {
                                    Image(systemName: "at")
                                        .font(.callout.weight(.semibold))
                                        .foregroundStyle(.tint)
                                        .frame(width: 20)
                                    TextField("Choose a username", text: $model.usernameInput)
                                        .textFieldStyle(.plain)
                                        .textContentType(.username)
                                        .focused($usernameFieldFocused)
                                        .onSubmit { model.authenticateUsername() }
                                }
                                .padding(.horizontal, 13)
                                .padding(.vertical, 11)
                                .background(Color.primary.opacity(0.055))
                                .clipShape(RoundedRectangle(cornerRadius: 11))
                                .overlay {
                                    RoundedRectangle(cornerRadius: 11)
                                        .stroke(usernameFieldFocused
                                                ? Color.accentColor
                                                : Color.primary.opacity(0.12),
                                                lineWidth: usernameFieldFocused ? 2 : 1)
                                }
                                Text("3–32 lowercase letters, numbers, or underscores")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }

                            Button {
                                model.authenticateUsername()
                            } label: {
                                HStack(spacing: 9) {
                                    Image(systemName: model.authMode == .register
                                          ? "person.badge.plus"
                                          : "arrow.right.circle.fill")
                                    Text(model.authMode == .register
                                         ? "Create secure account"
                                         : "Continue to Links")
                                }
                            }
                            .buttonStyle(.borderedProminent)
                            .controlSize(.regular)
                            .fixedSize(horizontal: true, vertical: false)
                            .disabled(model.isAuthenticating
                                      || model.usernameInput.trimmingCharacters(
                                        in: .whitespacesAndNewlines).isEmpty)

                            if model.isAuthenticating {
                                HStack(spacing: 8) {
                                    ProgressView()
                                        .controlSize(.small)
                                    Text("Securing your account…")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                .frame(maxWidth: .infinity)
                            }

                            if let error = model.onboardingError {
                                InlineAccountMessage(
                                    systemImage: "exclamationmark.triangle.fill",
                                    text: error,
                                    tint: .red)
                            }

                            if model.hasBoundAccount {
                                InlineAccountMessage(
                                    systemImage: "checkmark.shield.fill",
                                    text: "This profile is linked to \(model.accountStatus). You can sign in again or register a separate profile.",
                                    tint: .green)
                            }

                            HStack(spacing: 8) {
                                Image(systemName: "server.rack")
                                    .foregroundStyle(.secondary)
                                Text("Local account service")
                                    .font(.caption.weight(.medium))
                                Spacer()
                                Text(model.authEndpointText)
                                    .font(.system(.caption, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                    .truncationMode(.middle)
                            }
                        }
                        .padding(24)

                        Divider()

                        VStack(alignment: .leading, spacing: 12) {
                            Text("More ways to connect")
                                .font(.callout.weight(.semibold))
                            Text("Use these only if your account needs phone verification or device pairing.")
                                .font(.caption)
                                .foregroundStyle(.secondary)

                            AccountDisclosureRow(
                                title: "Use a phone number",
                                detail: "Verify with SMS or WhatsApp",
                                systemImage: "iphone",
                                tint: .orange,
                                isExpanded: $showingPhoneAccount) {
                                    PhoneAccountPanel(model: model)
                                }

                            AccountDisclosureRow(
                                title: "Join an existing account",
                                detail: "Create a secure pairing link",
                                systemImage: "person.2.fill",
                                tint: .purple,
                                isExpanded: $showingPairing) {
                                    ExistingAccountPanel(model: model)
                                }
                        }
                        .padding(20)
                    }
                    .frame(maxWidth: 640)
                    .background(.regularMaterial)
                    .clipShape(RoundedRectangle(cornerRadius: 20))
                    .overlay {
                        RoundedRectangle(cornerRadius: 20)
                            .stroke(Color.primary.opacity(0.11), lineWidth: 1)
                    }
                    .shadow(color: .black.opacity(0.14), radius: 28, y: 12)

                    HStack(spacing: 7) {
                        Image(systemName: "lock.fill")
                        Text("End-to-end encrypted by design")
                    }
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 28)
                .padding(.vertical, 36)
            }
        }
        .onAppear { usernameFieldFocused = true }
    }
}

private struct AccountOnboardingBackground: View {
    var body: some View {
        ZStack {
            Color(nsColor: .windowBackgroundColor)
            Circle()
                .fill(Color.blue.opacity(0.13))
                .frame(width: 520, height: 520)
                .blur(radius: 70)
                .offset(x: 300, y: -260)
            Circle()
                .fill(Color.purple.opacity(0.10))
                .frame(width: 440, height: 440)
                .blur(radius: 80)
                .offset(x: -360, y: 300)
        }
        .ignoresSafeArea()
    }
}

private struct AccountOnboardingHero: View {
    var body: some View {
        HStack(spacing: 16) {
            Image("LinksLogo")
                .resizable()
                .interpolation(.high)
                .scaledToFill()
                .clipShape(RoundedRectangle(cornerRadius: 17))
            .frame(width: 76, height: 76)
            .shadow(color: .blue.opacity(0.24), radius: 16, y: 7)

            VStack(alignment: .leading, spacing: 7) {
                Text("Welcome to Links")
                    .font(.system(size: 30, weight: .bold, design: .rounded))
                Text("Connect your account")
                    .font(.title3.weight(.semibold))
                Text("Private conversations. Simple setup.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: 640, alignment: .leading)
    }
}

private struct IconBadge: View {
    let systemImage: String
    let tint: Color

    var body: some View {
        Image(systemName: systemImage)
            .font(.title3.weight(.semibold))
            .foregroundStyle(tint)
            .frame(width: 38, height: 38)
            .background(tint.opacity(0.13))
            .clipShape(RoundedRectangle(cornerRadius: 11))
    }
}

private struct AccountModeButton: View {
    let mode: LinksMacOSAuthMode
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: mode == .register ? "person.badge.plus" : "arrow.right.circle")
                Text(mode == .register ? "Create account" : "Log in")
            }
            .font(.callout.weight(.semibold))
            .frame(maxWidth: .infinity)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(isSelected ? Color.white : Color.primary)
        .background {
            RoundedRectangle(cornerRadius: 8)
                .fill(isSelected ? Color.accentColor : Color.clear)
        }
    }
}

private struct InlineAccountMessage: View {
    let systemImage: String
    let text: String
    let tint: Color

    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            Image(systemName: systemImage)
                .foregroundStyle(tint)
            Text(text)
                .font(.caption)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 0)
        }
        .padding(11)
        .background(tint.opacity(0.09))
        .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct AccountDisclosureRow<Content: View>: View {
    let title: String
    let detail: String
    let systemImage: String
    let tint: Color
    @Binding var isExpanded: Bool
    let content: () -> Content

    var body: some View {
        DisclosureGroup(isExpanded: $isExpanded) {
            content()
                .padding(.top, 10)
        } label: {
            HStack(spacing: 11) {
                IconBadge(systemImage: systemImage, tint: tint)
                VStack(alignment: .leading, spacing: 3) {
                    Text(title)
                        .font(.callout.weight(.semibold))
                    Text(detail)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 8)
            }
        }
        .padding(13)
        .background(Color.primary.opacity(0.045))
        .clipShape(RoundedRectangle(cornerRadius: 13))
    }
}

private struct PhoneAccountPanel: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Phone verification is optional and needs a real HTTPS service with Twilio Verify.")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack(spacing: 9) {
                HStack(spacing: 9) {
                    Image(systemName: "phone.fill")
                        .foregroundStyle(.orange)
                    TextField("Phone number", text: $model.phoneInput)
                        .textFieldStyle(.plain)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 10)
                .background(Color.primary.opacity(0.055))
                .clipShape(RoundedRectangle(cornerRadius: 10))

                Picker("Channel", selection: $model.otpChannel) {
                    ForEach(IOSOTPChannel.allCases, id: \.self) { channel in
                        Text(channel.rawValue.capitalized).tag(channel)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(width: 112)
            }

            Button {
                model.startOTPEnrollment()
            } label: {
                Label("Send verification code", systemImage: "paperplane.fill")
            }
            .buttonStyle(.bordered)
            .disabled(!model.otpAvailable || model.isOTPWorking)

            if model.hasOTPChallenge {
                HStack(spacing: 9) {
                    HStack(spacing: 9) {
                        Image(systemName: "number")
                            .foregroundStyle(.orange)
                        TextField("Verification code", text: $model.otpCodeInput)
                            .textFieldStyle(.plain)
                            .textContentType(.oneTimeCode)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(Color.primary.opacity(0.055))
                    .clipShape(RoundedRectangle(cornerRadius: 10))

                    Button {
                        model.finishOTPEnrollment()
                    } label: {
                        Label("Verify", systemImage: "checkmark.circle.fill")
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.isOTPWorking)
                }
            }

            Text(model.otpAvailable
                 ? model.otpStatus
                 : "Phone verification is disabled for this local endpoint.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
    }
}

private struct ExistingAccountPanel: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Create a pairing link for an authenticated device, then use that link to join this account.")
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack(spacing: 9) {
                Image(systemName: "at")
                    .foregroundStyle(.purple)
                TextField("Account username or user ID", text: $model.pairingTarget)
                    .textFieldStyle(.plain)
            }
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
            .background(Color.primary.opacity(0.055))
            .clipShape(RoundedRectangle(cornerRadius: 10))

            Button {
                model.createPairingLink()
            } label: {
                Label("Create pairing link", systemImage: "link.badge.plus")
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
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.primary.opacity(0.055))
                    .clipShape(RoundedRectangle(cornerRadius: 9))
                Button {
                    model.copyPairingLink()
                } label: {
                    Label("Copy pairing link", systemImage: "doc.on.doc")
                }
                .buttonStyle(.bordered)
            }
        }
    }
}

private struct LinksMessagingView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @State private var showingNewConversation = false
    @State private var showingAddContact = false
    @State private var showingPairing = false
    @State private var showingNewGroup = false

    var body: some View {
        NavigationSplitView {
            LinksSidebar(model: model,
                         showingNewConversation: $showingNewConversation,
                         showingAddContact: $showingAddContact,
                         showingPairing: $showingPairing,
                         showingNewGroup: $showingNewGroup)
        } detail: {
            LinksConversationDetail(model: model)
        }
        .sheet(isPresented: $showingNewConversation) {
            NewConversationView(model: model)
        }
        .sheet(isPresented: $showingNewGroup) {
            NewGroupView(model: model)
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
    @Binding var showingNewGroup: Bool
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
                    showingNewGroup = true
                } label: {
                    Image(systemName: "person.3")
                }
                .buttonStyle(.borderless)
                .help("New group")
                .accessibilityLabel("New group")
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
                            ConversationRow(
                                conversation: conversation,
                                imageJPEG: model.pictureJPEG(for: conversation))
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
                            HStack(spacing: 8) {
                                Button {
                                    Task {
                                        await model.openSavedContact(userID: contact.userID)
                                    }
                                } label: {
                                    ContactRow(
                                        contact: contact,
                                        imageJPEG: model.contactPictures[contact.userID])
                                }
                                .buttonStyle(.plain)
                                Spacer(minLength: 0)
                                Button {
                                    contactToRemove = contact
                                } label: {
                                    Image(systemName: "trash")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                .buttonStyle(.borderless)
                                .help("Remove @\(contact.handle)")
                                .accessibilityLabel("Remove @\(contact.handle)")
                            }
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
            .onAppear {
                if let selectedConversationID = model.selectedConversationID {
                    model.markConversationRead(selectedConversationID)
                }
            }
            .onChange(of: model.selectedConversationID) { selectedConversationID in
                if let selectedConversationID {
                    model.markConversationRead(selectedConversationID)
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
    @State private var showingDisplayNameChange = false
    @State private var displayNameDraft = ""
    @State private var showingUsernameChange = false
    @State private var usernameDraft = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 11) {
            HStack(alignment: .top, spacing: 10) {
                Button(action: chooseProfilePicture) {
                    ProfileAvatar(title: model.profileDisplayName, size: 34,
                                  imageJPEG: model.profilePictureJPEG)
                }
                .buttonStyle(.plain)
                .help(model.profilePictureJPEG == nil ? "Add picture" : "Change picture")
                VStack(alignment: .leading, spacing: 4) {
                    Text(model.profileDisplayName)
                        .font(.callout.weight(.semibold))
                        .lineLimit(1)
                    Text(model.accountStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    HStack(spacing: 8) {
                        Button(model.profilePictureJPEG == nil ? "Add picture" : "Change picture",
                               action: chooseProfilePicture)
                        .buttonStyle(.bordered)
                        .controlSize(.small)
                        if model.profilePictureJPEG != nil {
                            Button("Remove picture", role: .destructive) {
                                model.removeProfilePicture()
                            }
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                }
                Spacer(minLength: 0)
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

            Button("Change display name") {
                displayNameDraft = model.profileDisplayName
                model.profileDisplayNameError = nil
                showingDisplayNameChange = true
            }
            .buttonStyle(.link)
            .controlSize(.small)
            .sheet(isPresented: $showingDisplayNameChange) {
                MacOSChangeDisplayNameSheet(model: model, displayName: $displayNameDraft)
            }

            Button("Change username") {
                usernameDraft = model.usernameInput
                model.usernameChangeError = nil
                showingUsernameChange = true
                Task {
                    if let current = await model.refreshCurrentUsername() {
                        usernameDraft = current
                    }
                }
            }
            .buttonStyle(.link)
            .controlSize(.small)
            .disabled(model.requiresAccountAuthentication)

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
        .sheet(isPresented: $showingUsernameChange) {
            MacOSChangeUsernameSheet(model: model, username: $usernameDraft)
        }
    }

    private func chooseProfilePicture() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.canChooseFiles = true
        panel.prompt = "Add"
        panel.message = "Choose a profile picture"
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let accessed = url.startAccessingSecurityScopedResource()
        defer { if accessed { url.stopAccessingSecurityScopedResource() } }
        guard let data = try? Data(contentsOf: url) else {
            model.actionError = "The picture could not be read."
            return
        }
        model.replaceProfilePicture(with: data)
    }
}

private struct MacOSChangeUsernameSheet: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var username: String
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Change username")
                .font(.title2.weight(.semibold))

            TextField("New username", text: $username)
                .textFieldStyle(.roundedBorder)
                .textContentType(.username)
                .autocorrectionDisabled()

            Text("Use 3–32 lowercase letters, numbers, or underscores. Your old username becomes available to others.")
                .font(.caption)
                .foregroundStyle(.secondary)

            if let error = model.usernameChangeError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button {
                    Task {
                        if await model.changeUsername(to: username) {
                            dismiss()
                        }
                    }
                } label: {
                    if model.isChangingUsername {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Text("Save")
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || model.isChangingUsername)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(22)
        .frame(width: 390)
    }
}

private struct MacOSChangeDisplayNameSheet: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Binding var displayName: String
    @Environment(\.dismiss) private var dismiss
    @State private var isSaving = false

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Change display name")
                .font(.title2.weight(.semibold))

            TextField("Display name", text: $displayName)
                .textFieldStyle(.roundedBorder)
                .autocorrectionDisabled()

            Text("Shown to people in your contacts and conversations.")
                .font(.caption)
                .foregroundStyle(.secondary)

            if let error = model.profileDisplayNameError {
                Text(error)
                    .font(.caption)
                    .foregroundStyle(.red)
                    .fixedSize(horizontal: false, vertical: true)
            }

            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Save") {
                    Task {
                        isSaving = true
                        let saved = await model.changeProfileDisplayName(to: displayName)
                        isSaving = false
                        if saved { dismiss() }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(displayName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isSaving)
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(22)
        .frame(width: 390)
    }
}

private struct ProfileAvatar: View {
    let title: String
    let size: CGFloat
    var imageJPEG: Data? = nil

    var body: some View {
        Group {
            if let imageJPEG, let image = NSImage(data: imageJPEG) {
                Image(nsImage: image)
                    .resizable()
                    .scaledToFill()
            } else {
                Text(String(title.trimmingCharacters(in: CharacterSet(charactersIn: "@ ")).prefix(1)).uppercased())
                    .font(.system(size: size * 0.42, weight: .semibold))
                    .foregroundStyle(.tint)
            }
        }
        .frame(width: size, height: size)
        .background(Color.accentColor.opacity(0.14))
        .clipShape(Circle())
    }
}

private struct ConversationRow: View {
    let conversation: LinksMacOSConversation
    var imageJPEG: Data? = nil

    var body: some View {
        HStack(spacing: 10) {
            if conversation.isGroup {
                Image(systemName: "person.3.fill")
                    .font(.caption)
                    .foregroundStyle(.tint)
                    .frame(width: 30, height: 30)
                    .background(Color.accentColor.opacity(0.15))
                    .clipShape(Circle())
            } else {
                ProfileAvatar(title: conversation.displayTitle, size: 30, imageJPEG: imageJPEG)
            }
            VStack(alignment: .leading, spacing: 3) {
                Text(conversation.displayTitle)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Text(conversation.isGroup
                     ? (conversation.messages.last?.text ?? "No messages")
                     : conversation.title)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                if !conversation.isGroup {
                    Text(conversation.messages.last?.text ?? "No messages")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            if conversation.unreadCount > 0 {
                Text(conversation.unreadCount > 99 ? "99+" : "\(conversation.unreadCount)")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 3)
                    .background(Color.accentColor)
                    .clipShape(Capsule())
            }
        }
        .padding(.vertical, 3)
    }
}

private struct ContactRow: View {
    let contact: LinksMacOSContact
    var imageJPEG: Data? = nil

    var body: some View {
        HStack(spacing: 10) {
            ProfileAvatar(title: contact.displayTitle, size: 30, imageJPEG: imageJPEG)
            VStack(alignment: .leading, spacing: 3) {
                Text(contact.displayTitle)
                    .font(.callout.weight(.medium))
                    .lineLimit(1)
                Text("@\(contact.handle) · " + (contact.deviceCount == 1
                     ? "1 active device"
                     : "\(contact.deviceCount) active devices"))
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
    @State private var showingGroupMembers = false

    private func groupSubtitle(_ conversation: LinksMacOSConversation) -> String {
        guard conversation.groupActive else { return "Group · you are no longer a member" }
        let count = model.groupMembers.count
        return count == 0 ? "Encrypted group" : "Encrypted group · \(count) members"
    }

    var body: some View {
        if let conversation = model.selectedConversation {
            VStack(spacing: 0) {
                HStack(spacing: 12) {
                    ProfileAvatar(
                        title: conversation.displayTitle,
                        size: 38,
                        imageJPEG: model.pictureJPEG(for: conversation))
                    VStack(alignment: .leading, spacing: 3) {
                        Text(conversation.displayTitle)
                            .font(.title2.weight(.semibold))
                        Text(conversation.isGroup
                             ? groupSubtitle(conversation)
                             : conversation.title)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 7) {
                        HStack(spacing: 7) {
                            if conversation.isGroup {
                                Button {
                                    showingGroupMembers = true
                                } label: {
                                    Label("Members", systemImage: "person.3")
                                }
                                .buttonStyle(.bordered)
                                .controlSize(.small)
                                .help("Group members")
                            } else {
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
                        }
                        StatusPill(title: model.connectionStatus,
                                   color: linksStatusColor(model.connectionStatus))
                    }
                }
                .padding(.horizontal, 22)
                .padding(.vertical, 14)

                if model.deliveryState != .ready {
                    DeliveryStatusBanner(model: model)
                }

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
                MessageList(messages: conversation.messages,
                            conversationID: conversation.id,
                            messageImages: model.messageImages,
                            senderLabel: { model.senderLabel(for: $0) })
                Divider()
                ComposerView(model: model)
            }
            .background(Color.primary.opacity(0.015))
            .sheet(isPresented: $showingGroupMembers) {
                GroupMembersView(model: model)
            }
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
    let conversationID: String
    let messageImages: [UUID: NSImage]
    var senderLabel: (LinksMacOSMessage) -> String? = { _ in nil }

    private let messageListBottomID = "message-list-bottom"

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
                            MessageBubble(
                                message: message,
                                senderLabel: senderLabel(message),
                                image: messageImages[message.id])
                                .id(message.id)
                        }
                    }
                    Color.clear
                        .frame(height: 1)
                        .id(messageListBottomID)
                }
                .padding(24)
            }
            .onAppear {
                DispatchQueue.main.async {
                    proxy.scrollTo(messageListBottomID, anchor: .bottom)
                }
            }
            .onChange(of: conversationID) { _ in
                DispatchQueue.main.async {
                    proxy.scrollTo(messageListBottomID, anchor: .bottom)
                }
            }
            .onChange(of: messages.count) { _ in
                DispatchQueue.main.async {
                    withAnimation {
                        proxy.scrollTo(messageListBottomID, anchor: .bottom)
                    }
                }
            }
        }
    }
}

private struct MessageBubble: View {
    let message: LinksMacOSMessage
    var senderLabel: String? = nil
    var image: NSImage? = nil

    var body: some View {
        HStack(alignment: .bottom) {
            if message.isOutgoing { Spacer(minLength: 90) }
            VStack(alignment: message.isOutgoing ? .trailing : .leading, spacing: 4) {
                if let senderLabel {
                    Text(senderLabel)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                if message.imageMetadataProtobuf != nil {
                    if let image {
                        Image(nsImage: image)
                            .resizable()
                            .aspectRatio(contentMode: .fit)
                            .frame(maxWidth: 440, maxHeight: 440)
                            .clipShape(RoundedRectangle(cornerRadius: 13))
                    } else {
                        Label("Loading image…", systemImage: "photo")
                            .font(.body)
                            .padding(.horizontal, 13)
                            .padding(.vertical, 9)
                            .background(Color.primary.opacity(0.08))
                            .clipShape(RoundedRectangle(cornerRadius: 13))
                    }
                } else {
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
                }
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

private struct ComposerTextEditor: NSViewRepresentable {
    @Binding var text: String
    var isEnabled: Bool
    var onPasteImage: (NSImage) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = ImagePastingTextView()
        textView.delegate = context.coordinator
        textView.isRichText = false
        textView.isVerticallyResizable = true
        textView.isHorizontallyResizable = false
        textView.drawsBackground = false
        textView.textColor = .labelColor
        textView.font = .systemFont(ofSize: NSFont.systemFontSize)
        textView.textContainerInset = NSSize(width: 0, height: 5)
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.widthTracksTextView = true
        textView.textContainer?.containerSize = NSSize(
            width: CGFloat.greatestFiniteMagnitude,
            height: CGFloat.greatestFiniteMagnitude)

        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.borderType = .noBorder
        scrollView.hasHorizontalScroller = false
        scrollView.hasVerticalScroller = true
        scrollView.documentView = textView
        configurePasteHandler(for: textView, coordinator: context.coordinator)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = scrollView.documentView as? ImagePastingTextView else { return }
        if textView.string != text {
            textView.string = text
        }
        textView.isEditable = isEnabled
        textView.isSelectable = isEnabled
        configurePasteHandler(for: textView, coordinator: context.coordinator)
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: NSScrollView,
        context: Context
    ) -> CGSize? {
        let width = proposal.width ?? 320
        guard let textView = nsView.documentView as? NSTextView,
              let textContainer = textView.textContainer,
              let layoutManager = textView.layoutManager else {
            return CGSize(width: width, height: 38)
        }
        textContainer.containerSize = NSSize(
            width: max(1, width),
            height: .greatestFiniteMagnitude)
        layoutManager.ensureLayout(for: textContainer)
        let contentHeight = layoutManager.usedRect(for: textContainer).height
            + textView.textContainerInset.height * 2
        return CGSize(width: width, height: min(108, max(36, ceil(contentHeight))))
    }

    private func configurePasteHandler(
        for textView: ImagePastingTextView,
        coordinator: Coordinator
    ) {
        textView.onPasteImage = { [weak coordinator] in
            guard let coordinator,
                  let image = NSImage(pasteboard: .general) else { return false }
            coordinator.parent.onPasteImage(image)
            return true
        }
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerTextEditor

        init(_ parent: ComposerTextEditor) {
            self.parent = parent
        }

        func textDidChange(_ notification: Notification) {
            guard let textView = notification.object as? NSTextView else { return }
            parent.text = textView.string
        }
    }
}

private final class ImagePastingTextView: NSTextView {
    var onPasteImage: (() -> Bool)?

    override func paste(_ sender: Any?) {
        if onPasteImage?() == true { return }
        super.paste(sender)
    }
}

private struct ComposerView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            if let preview = model.composerImagePreview {
                HStack(spacing: 10) {
                    Image(nsImage: preview)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: 64, height: 64)
                        .clipShape(RoundedRectangle(cornerRadius: 8))
                    Text("Image ready to send")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button {
                        model.clearComposerImage()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                    }
                    .buttonStyle(.plain)
                    .disabled(model.isSendingComposerImage)
                    .help("Remove pasted image")
                }
                .padding(.horizontal, 4)
            }
            HStack(alignment: .bottom, spacing: 10) {
                ZStack(alignment: .leading) {
                    ComposerTextEditor(
                        text: $model.composerText,
                        isEnabled: model.canComposeSelectedConversation
                            && !model.isSendingComposerImage,
                        onPasteImage: model.setComposerImage)
                        .frame(maxWidth: .infinity, minHeight: 36, maxHeight: 108)
                    if model.composerText.isEmpty {
                        Text("Message · paste an image")
                            .foregroundStyle(.secondary)
                            .padding(.horizontal, 12)
                            .allowsHitTesting(false)
                    }
                }
                .padding(.horizontal, 12)
                .background(Color.primary.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 10))
                Button {
                    model.pasteComposerImageFromClipboard()
                } label: {
                    Image(systemName: "photo.badge.plus")
                }
                .buttonStyle(.plain)
                .help("Paste image from clipboard")
                .disabled(!model.canComposeSelectedConversation
                          || model.isSendingComposerImage)
                Button {
                    if model.composerImageData != nil {
                        model.sendComposerImage()
                    } else {
                        model.sendMessage()
                    }
                } label: {
                    if model.isSendingComposerImage {
                        ProgressView()
                            .controlSize(.small)
                    } else {
                        Image(systemName: "arrow.up.circle.fill")
                            .font(.title2.weight(.semibold))
                    }
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
                .keyboardShortcut(.return, modifiers: [.command])
                .help("Send message or image")
                .disabled(!model.canComposeSelectedConversation
                          || model.isSendingComposerImage
                          || (model.composerText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                              && model.composerImageData == nil)
                          || (model.composerImageData != nil && !model.canSendComposerImage))
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background(.bar)
    }
}

private struct NewConversationView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var handle = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New conversation")
                .font(.title2.weight(.semibold))
            Text("Find someone by username. The app resolves the recipient ID automatically.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Username, for example karine", text: $handle)
                .textFieldStyle(.roundedBorder)
                .textContentType(.username)
                .onSubmit { createConversation() }
            if !model.conversationCreationStatus.isEmpty {
                Text(model.conversationCreationStatus)
                    .font(.caption)
                    .foregroundStyle(model.conversationCreationStatus.contains("Could not")
                                     || model.conversationCreationStatus.contains("No Links")
                                     || model.conversationCreationStatus.contains("Choose")
                                     ? .red : .secondary)
            }
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create", action: createConversation)
                .buttonStyle(.borderedProminent)
                .disabled(handle.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || model.isCreatingConversation)
            }
        }
        .padding(24)
        .frame(width: 430)
    }

    private func createConversation() {
        Task {
            if await model.createConversation(handle: handle) {
                dismiss()
            }
        }
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
                .onSubmit { addContact() }
            Text(model.contactStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
            HStack {
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Find and add", action: addContact)
                .buttonStyle(.borderedProminent)
                .disabled(model.isAddingContact)
            }
        }
        .padding(24)
        .frame(width: 460)
    }

    private func addContact() {
        Task {
            if await model.addContact(handle: handle) {
                dismiss()
            }
        }
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

private struct ContactPicker: View {
    let contacts: [LinksMacOSContact]
    @Binding var selection: Set<String>

    var body: some View {
        if contacts.isEmpty {
            Text("Add contacts first, then invite them here.")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            ScrollView {
                VStack(alignment: .leading, spacing: 6) {
                    ForEach(contacts) { contact in
                        Toggle(isOn: Binding(
                            get: { selection.contains(contact.userID) },
                            set: { isOn in
                                if isOn { selection.insert(contact.userID) } else { selection.remove(contact.userID) }
                            })) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(contact.displayTitle)
                                Text("@\(contact.handle)")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .toggleStyle(.checkbox)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 180)
        }
    }
}

private struct NewGroupView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var selection = Set<String>()

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("New group")
                .font(.title2.weight(.semibold))
            Text("Messages are end-to-end encrypted. The server stores who is in the group, never its name or messages.")
                .font(.caption)
                .foregroundStyle(.secondary)
            TextField("Group name", text: $name)
                .textFieldStyle(.roundedBorder)
            Text("Invite")
                .font(.headline)
            ContactPicker(contacts: model.contacts, selection: $selection)
            if !model.groupStatus.isEmpty {
                Text(model.groupStatus)
                    .font(.caption)
                    .foregroundStyle(.red)
            }
            HStack {
                if model.isUpdatingGroup {
                    ProgressView().controlSize(.small)
                }
                Spacer()
                Button("Cancel") { dismiss() }
                Button("Create") {
                    Task {
                        if await model.createGroup(name: name, memberUserIDs: Array(selection)) {
                            dismiss()
                        }
                    }
                }
                .buttonStyle(.borderedProminent)
                .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                          || selection.isEmpty || model.isUpdatingGroup)
            }
        }
        .padding(24)
        .frame(width: 430)
    }
}

private struct GroupMembersView: View {
    @ObservedObject var model: LinksMacOSAppModel
    @Environment(\.dismiss) private var dismiss
    @State private var selection = Set<String>()
    @State private var name = ""
    @State private var memberToRemove: LinksMacOSGroupMember?
    @State private var memberToOwn: LinksMacOSGroupMember?
    @State private var leaveConfirmationPresented = false
    @State private var disbandConfirmationPresented = false

    private var invitableContacts: [LinksMacOSContact] {
        let members = Set(model.groupMembers.map(\.userID))
        return model.contacts.filter { !members.contains($0.userID) }
    }

    private func canRemove(_ member: LinksMacOSGroupMember) -> Bool {
        guard model.canManageSelectedGroup, !member.isSelf, member.role != .owner else { return false }
        return model.selectedGroupRole == .owner || member.role != .admin
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text(model.selectedConversation?.title ?? "Group")
                    .font(.title2.weight(.semibold))
                Spacer()
                if model.isUpdatingGroup { ProgressView().controlSize(.small) }
            }
            if model.canManageSelectedGroup {
                HStack {
                    TextField("Group name", text: $name)
                        .textFieldStyle(.roundedBorder)
                    Button("Rename") {
                        Task { await model.renameSelectedGroup(name) }
                    }
                    .disabled(name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                              || name == model.selectedConversation?.title)
                }
            }
            Text("Members")
                .font(.headline)
            ScrollView {
                VStack(alignment: .leading, spacing: 8) {
                    ForEach(model.groupMembers) { member in
                        HStack {
                            Text(member.isSelf ? "\(member.displayName) (you)" : member.displayName)
                            if let role = member.role, role != .member {
                                Text(role == .owner ? "Owner" : "Admin")
                                    .font(.caption2.weight(.semibold))
                                    .padding(.horizontal, 6)
                                    .padding(.vertical, 2)
                                    .background(Color.accentColor.opacity(0.15))
                                    .clipShape(Capsule())
                            }
                            Spacer()
                            if model.selectedGroupRole == .owner, !member.isSelf, member.role != .owner {
                                Button("Make owner") {
                                    memberToOwn = member
                                }
                                .controlSize(.small)
                            }
                            if model.selectedGroupRole == .owner, !member.isSelf, member.role == .member {
                                Button("Make admin") {
                                    Task { await model.makeAdminInSelectedGroup(member.userID) }
                                }
                                .controlSize(.small)
                            }
                            if canRemove(member) {
                                Button {
                                    memberToRemove = member
                                } label: {
                                    Image(systemName: "person.badge.minus")
                                }
                                .buttonStyle(.borderless)
                                .help("Remove \(member.displayName)")
                            }
                        }
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 200)
            if model.canManageSelectedGroup {
                Text("Add people")
                    .font(.headline)
                ContactPicker(contacts: invitableContacts, selection: $selection)
                HStack {
                    Spacer()
                    Button("Add") {
                        let chosen = Array(selection)
                        selection.removeAll()
                        Task { await model.addMembersToSelectedGroup(chosen) }
                    }
                    .disabled(selection.isEmpty || model.isUpdatingGroup)
                }
            }
            if !model.groupStatus.isEmpty {
                Text(model.groupStatus)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            HStack {
                if model.selectedConversation?.groupActive == true {
                    if model.selectedGroupRole == .owner {
                        Button("Disband group", role: .destructive) {
                            disbandConfirmationPresented = true
                        }
                    } else {
                        Button("Leave group", role: .destructive) {
                            leaveConfirmationPresented = true
                        }
                    }
                }
                Spacer()
                Button("Done") { dismiss() }
                    .buttonStyle(.borderedProminent)
            }
        }
        .padding(24)
        .frame(width: 440)
        .onAppear {
            name = model.selectedConversation?.title ?? ""
            Task { await model.refreshSelectedGroupMembers() }
        }
        .alert("Remove \(memberToRemove?.displayName ?? "member")?",
               isPresented: Binding(get: { memberToRemove != nil },
                                    set: { if !$0 { memberToRemove = nil } })) {
            Button("Remove", role: .destructive) {
                if let member = memberToRemove {
                    Task { await model.removeFromSelectedGroup(member.userID) }
                }
                memberToRemove = nil
            }
            Button("Cancel", role: .cancel) { memberToRemove = nil }
        } message: {
            Text("They stop receiving new messages. Messages they already have stay on their device.")
        }
        .alert("Make \(memberToOwn?.displayName ?? "this member") the owner?",
               isPresented: Binding(get: { memberToOwn != nil },
                                    set: { if !$0 { memberToOwn = nil } })) {
            Button("Make owner") {
                if let member = memberToOwn {
                    Task { await model.giveOwnershipInSelectedGroup(to: member.userID) }
                }
                memberToOwn = nil
            }
            Button("Cancel", role: .cancel) { memberToOwn = nil }
        } message: {
            Text("You become a member. The owner can disband the group, and you can leave it.")
        }
        .alert("Disband this group?", isPresented: $disbandConfirmationPresented) {
            Button("Disband", role: .destructive) {
                Task {
                    await model.disbandSelectedGroup()
                    if model.selectedConversation == nil { dismiss() }
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("Everyone is removed and the group is deleted. Messages already on their devices stay there.")
        }
        .alert("Leave this group?", isPresented: $leaveConfirmationPresented) {
            Button("Leave", role: .destructive) {
                Task {
                    await model.leaveSelectedGroup()
                    dismiss()
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("You leave the group and it disappears from your chats.")
        }
    }
}
