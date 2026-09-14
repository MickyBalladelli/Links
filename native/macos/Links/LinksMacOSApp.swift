import SwiftUI
import LinksClient
import LinksKeyStore

@main
struct LinksMacOSApp: App {
    @StateObject private var model = LinksMacOSAppModel()

    var body: some Scene {
        WindowGroup("Links") {
            LinksRootView(model: model)
                .frame(minWidth: 760, minHeight: 480)
        }
    }
}

@MainActor
final class LinksMacOSAppModel: ObservableObject {
    @Published private(set) var identityStatus = "Checking identity"
    @Published private(set) var accountStatus = "Signed out"
    @Published private(set) var connectionStatus = "Offline"
    @Published private(set) var detail = "The macOS host is ready for the shared client integration."

    private let client: IOSClient?
    private let keyStore = HardwareIdentityStore()

    init() {
        do {
            let loadedClient = try IOSClient()
            client = loadedClient
            identityStatus = loadedClient.isEnrolled ? "Identity enrolled" : "Identity not enrolled"
            accountStatus = loadedClient.isAuthenticated ? "Authenticated" : "Signed out"
            detail = "Secure identity custody is provided by LinksKeyStore."
        } catch {
            client = nil
            identityStatus = "Identity unavailable"
            detail = "Saved identity metadata could not be restored."
        }
    }

    var packageStatus: String {
        _ = keyStore
        _ = client
        return "LinksClient + LinksKeyStore"
    }
}

private struct LinksRootView: View {
    @ObservedObject var model: LinksMacOSAppModel

    var body: some View {
        HStack(spacing: 0) {
            sidebar
            Divider()
            conversationPlaceholder
        }
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var sidebar: some View {
        VStack(alignment: .leading, spacing: 18) {
            Label("Links", systemImage: "lock.shield")
                .font(.title2.weight(.semibold))

            VStack(alignment: .leading, spacing: 10) {
                StatusRow(title: "Identity", value: model.identityStatus)
                StatusRow(title: "Account", value: model.accountStatus)
                StatusRow(title: "Connection", value: model.connectionStatus)
            }

            Spacer()

            Text(model.packageStatus)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .padding(22)
        .frame(width: 240, maxHeight: .infinity, alignment: .topLeading)
    }

    private var conversationPlaceholder: some View {
        VStack(spacing: 14) {
            Image(systemName: "bubble.left.and.bubble.right")
                .font(.system(size: 42))
                .foregroundStyle(.secondary)

            Text("Welcome to Links")
                .font(.title2.weight(.semibold))

            Text(model.detail)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 420)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(40)
    }
}

private struct StatusRow: View {
    let title: String
    let value: String

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
            Text(value)
                .font(.body)
        }
    }
}
