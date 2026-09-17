import Foundation
@preconcurrency import LinksClient
import LinksKeyStore
import SwiftUI
import UIKit

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

@MainActor
final class IOSMobileAppModel: ObservableObject {
    @Published private(set) var status = "Opening secure identity store"
    @Published private(set) var identityStatus = "No identity enrolled"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var deviceStatus = "No device"
    @Published private(set) var isBusy = false
    @Published private(set) var isEnrolled = false
    @Published private(set) var isAuthenticated = false
    @Published private(set) var pairingStatus = "No pairing activity"
    @Published var error: String?
    @Published var username = ""
    @Published var usernameAction: IOSUsernameAction = .register
    @Published var phone = ""
    @Published var verificationCode = ""
    @Published var channel: IOSOTPChannel = .sms
    @Published var pairingInput = ""

    let authEndpointText: String
    private let client: IOSClient?
    private let usernameAuthClient: IOSUsernameAuthClient?
    private let otpClient: IOSOTPClient?
    private var otpChallenge: IOSOTPChallenge?

    init() {
        let endpointText = Bundle.main.object(forInfoDictionaryKey: "LINKS_AUTH_URL") as? String
            ?? "https://api.links.invalid"
        authEndpointText = endpointText

        var loadedClient: IOSClient?
        var loadedUsernameAuthClient: IOSUsernameAuthClient?
        var loadedOTPClient: IOSOTPClient?
        var initialError: String?
        do {
            guard let endpoint = URL(string: endpointText) else {
                throw IOSUsernameAuthError.invalidEndpoint
            }
            loadedClient = try IOSClient(
                identityStore: HardwareIdentityStore(),
                defaults: .standard)
            loadedUsernameAuthClient = try IOSUsernameAuthClient(baseURL: endpoint)
            loadedOTPClient = try IOSOTPClient(baseURL: endpoint)
        } catch {
            initialError = "Mobile client could not open its identity store."
        }
        client = loadedClient
        usernameAuthClient = loadedUsernameAuthClient
        otpClient = loadedOTPClient
        error = initialError
        if initialError != nil {
            status = "Identity store unavailable"
        } else {
            refreshState()
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

    private func startUsernameAuthentication(
        using client: IOSClient,
        api: IOSUsernameAuthClient,
        handle: String,
        action: IOSUsernameAction) {
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
                self.status = action == .register ? "Username registered" : "Account authenticated"
                self.refreshState()
            } catch let usernameError as IOSUsernameAuthError {
                guard let self else { return }
                self.isBusy = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = self.usernameErrorMessage(usernameError, action: action)
            } catch {
                guard let self else { return }
                self.isBusy = false
                self.status = action == .register ? "Username registration failed" : "Username login failed"
                self.error = "The local identity could not complete the request. Check the profile and auth service."
            }
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
            return "Username already exists. Choose another username."
        case .rateLimited(let retryAfterSeconds):
            if let retryAfterSeconds {
                return "Too many requests. Try again in \(retryAfterSeconds) seconds."
            }
            return "Too many requests. Try again later."
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

    func clearError() {
        error = nil
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
    }
}

struct IOSMobileRootView: View {
    @ObservedObject var model: IOSMobileAppModel

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Label("Links mobile", systemImage: "lock.shield")
                        .font(.title2.weight(.semibold))
                    Text("Internal physical-device build")
                        .foregroundStyle(.secondary)
                    Text(model.status)
                        .foregroundStyle(model.error == nil ? Color.secondary : Color.red)
                }

                Section("Identity") {
                    LabeledContent("State", value: model.identityStatus)
                    LabeledContent("Device", value: model.deviceStatus)
                    if !model.isEnrolled {
                        Button("Create hardware identity") {
                            model.createIdentity()
                        }
                        .disabled(model.isBusy)
                    }
                }

                Section("Local development account") {
                    Text("No SMS needed. The local backend creates one account per username.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Text(model.isEnrolled
                         ? "Your hardware identity signs this request."
                         : "First registration creates your secure hardware identity automatically.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Picker("Action", selection: $model.usernameAction) {
                        ForEach(IOSUsernameAction.allCases) { action in
                            Text(action.title).tag(action)
                        }
                    }
                    .pickerStyle(.segmented)
                    TextField("Username, for example alice", text: $model.username)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .textContentType(.username)
                    Button(model.usernameAction == .register ? "Register username" : "Log in") {
                        model.authenticateUsername()
                    }
                    .disabled(model.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.isBusy)
                }

                Section("Phone account") {
                    Text("Use this only with a real HTTPS account-auth service configured for Twilio Verify. Local development uses username login above.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    LabeledContent("Account", value: model.accountStatus)
                    Text("Auth service: \(model.authEndpointText)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextField("Phone, for example +33123456789", text: $model.phone)
                        .keyboardType(.phonePad)
                        .textContentType(.telephoneNumber)
                    Picker("Channel", selection: $model.channel) {
                        Text("SMS").tag(IOSOTPChannel.sms)
                        Text("WhatsApp").tag(IOSOTPChannel.whatsapp)
                    }
                    .pickerStyle(.segmented)
                    Button("Send verification code") {
                        model.sendVerificationCode()
                    }
                    .disabled(!model.isEnrolled || model.isBusy)
                    SecureField("Verification code", text: $model.verificationCode)
                        .keyboardType(.numberPad)
                        .textContentType(.oneTimeCode)
                    Button("Verify phone") {
                        model.verifyCode()
                    }
                    .disabled(model.verificationCode.isEmpty || model.isBusy)
                }

                Section("Approve another device") {
                    Text("Paste a signed links://connect link. Review it before approval.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    TextEditor(text: $model.pairingInput)
                        .font(.system(.footnote, design: .monospaced))
                        .frame(minHeight: 120)
                    Button("Approve device") {
                        model.approvePairing()
                    }
                    .disabled(!model.isAuthenticated || model.isBusy)
                    Text(model.pairingStatus)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .simultaneousGesture(
                TapGesture().onEnded {
                    dismissKeyboard()
                })
            .navigationTitle("Links")
            .overlay(alignment: .bottom) {
                if let error = model.error {
                    HStack(alignment: .top, spacing: 10) {
                        Image(systemName: "exclamationmark.circle.fill")
                            .foregroundStyle(.red)
                        Text(error)
                            .font(.footnote)
                            .frame(maxWidth: .infinity, alignment: .leading)
                        Button("Dismiss") {
                            model.clearError()
                        }
                        .font(.footnote.weight(.semibold))
                    }
                    .padding()
                    .background(.thinMaterial)
                }
            }
        }
    }

    private func dismissKeyboard() {
        UIApplication.shared.sendAction(
            #selector(UIResponder.resignFirstResponder),
            to: nil,
            from: nil,
            for: nil)
    }
}
