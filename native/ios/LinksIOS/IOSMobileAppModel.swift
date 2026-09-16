import Foundation
@preconcurrency import LinksClient
import LinksKeyStore
import SwiftUI

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
            accountStatus = client.isAuthenticated
                ? "Authenticated · \(String(userID.prefix(8)))"
                : "Account saved · sign in again"
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

                Section("Phone account") {
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
}
