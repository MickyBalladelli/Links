import AppKit
import SwiftUI

struct LinksAdminRootView: View {
    @ObservedObject var model: LinksAdminModel

    var body: some View {
        Group {
            if model.isConnected {
                LinksAdminDashboard(model: model)
            } else {
                LinksAdminLoginView(model: model)
            }
        }
        .alert("Needs attention", isPresented: Binding(
            get: { model.errorMessage != nil },
            set: { if !$0 { model.errorMessage = nil } })) {
            Button("OK") { model.errorMessage = nil }
        } message: {
            Text(model.errorMessage ?? "")
        }
    }
}

private struct LinksAdminLoginView: View {
    @ObservedObject var model: LinksAdminModel

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            LinksAdminIconView(size: 72)
            Text("Links Admin")
                .font(.largeTitle.weight(.bold))
            Text("Manage accounts and devices. The admin key stays in memory and is never saved by this app.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            TextField("Account service URL", text: $model.endpointText)
                .textFieldStyle(.roundedBorder)
            SecureField("Admin key", text: $model.adminKey)
                .textFieldStyle(.roundedBorder)
                .onSubmit { model.connect() }
            HStack {
                Spacer()
                Button("Connect") { model.connect() }
                    .buttonStyle(.borderedProminent)
                    .disabled(model.adminKey.isEmpty || model.isLoading)
            }
        }
        .padding(34)
        .frame(width: 480)
    }
}

private struct LinksAdminDashboard: View {
    @ObservedObject var model: LinksAdminModel

    var body: some View {
        NavigationSplitView {
            VStack(spacing: 0) {
                HStack {
                    LinksAdminIconView(size: 32)
                    Text("Users")
                        .font(.title2.weight(.semibold))
                    Spacer()
                    Button {
                        Task { await model.refresh() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                    .buttonStyle(.borderless)
                    .disabled(model.isLoading)
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 12)

                TextField("Search username or UUID", text: $model.searchText)
                    .textFieldStyle(.roundedBorder)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 10)

                List(model.filteredUsers, selection: $model.selectedUserID) { user in
                    LinksAdminUserRow(user: user)
                        .tag(user.id)
                }
            }
            .navigationSplitViewColumnWidth(min: 260, ideal: 300)
        } detail: {
            if let user = model.selectedUser {
                LinksAdminUserDetail(model: model, account: user)
            } else {
                VStack(spacing: 10) {
                    Image(systemName: "person.crop.circle")
                        .font(.system(size: 34))
                        .foregroundStyle(.secondary)
                    Text("No user selected")
                        .foregroundStyle(.secondary)
                }
            }
        }
    }
}

private struct LinksAdminIconView: View {
    let size: CGFloat

    var body: some View {
        Group {
            if let iconURL = Bundle.main.url(forResource: "icon-admin", withExtension: "png"),
               let icon = NSImage(contentsOf: iconURL) {
                Image(nsImage: icon)
                    .resizable()
                    .scaledToFit()
            } else {
                Image(systemName: "person.2.badge.gearshape.fill")
                    .foregroundStyle(.tint)
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size * 0.22))
    }
}

private struct LinksAdminUserRow: View {
    let user: LinksAdminUser

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: user.isDisabled ? "person.crop.circle.badge.xmark" : "person.crop.circle.fill")
                .foregroundStyle(user.isDisabled ? Color.red : Color.accentColor)
            VStack(alignment: .leading, spacing: 3) {
                Text(user.displayName)
                    .font(.headline)
                Text("\(user.activeDeviceCount) active device\(user.activeDeviceCount == 1 ? "" : "s")")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct LinksAdminUserDetail: View {
    @ObservedObject var model: LinksAdminModel
    let account: LinksAdminUser

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                HStack(alignment: .top) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text(account.displayName)
                            .font(.largeTitle.weight(.bold))
                        Text(account.accountKind.capitalized)
                            .foregroundStyle(.secondary)
                    }
                    Spacer()
                    Label(account.isDisabled ? "Disabled" : "Active",
                          systemImage: account.isDisabled ? "pause.circle.fill" : "checkmark.circle.fill")
                        .foregroundStyle(account.isDisabled ? .red : .green)
                }

                GroupBox("Account") {
                    VStack(alignment: .leading, spacing: 10) {
                        LabeledContent("User ID", value: account.userID.uuidString)
                        LabeledContent("Created", value: account.createdAt)
                        if let disabledAt = account.disabledAt {
                            LabeledContent("Disabled", value: disabledAt)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
                }

                GroupBox("Devices") {
                    VStack(alignment: .leading, spacing: 0) {
                        if account.devices.isEmpty {
                            Text("No devices")
                                .foregroundStyle(.secondary)
                        } else {
                            Text(account.devices.map { device in
                                let state = device.isActive ? "Active" : "Revoked"
                                return "\(device.deviceID.uuidString) · \(state) · registered \(device.registeredAt)"
                            }.joined(separator: "\n"))
                                .font(.system(.body, design: .monospaced))
                                .textSelection(.enabled)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }

                HStack {
                    Spacer()
                    Button(account.isDisabled ? "Enable account" : "Disable account") {
                        Task { await model.setUserDisabled(account, disabled: !account.isDisabled) }
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(account.isDisabled ? .green : .red)
                }
            }
            .padding(28)
            .frame(maxWidth: 820, alignment: .leading)
        }
        .navigationTitle("User details")
    }
}
