import SwiftUI
import AppKit

@main
struct LinksMacOSApp: App {
    @StateObject private var profileSession = LinksMacOSProfileSession()
    @NSApplicationDelegateAdaptor(LinksMacOSApplicationDelegate.self)
    private var applicationDelegate
    @Environment(\.scenePhase) private var scenePhase

    private func applyApplicationIcon() {
        guard let iconURL = Bundle.main.url(forResource: "AppIcon", withExtension: "icns"),
              let icon = NSImage(contentsOf: iconURL) else { return }
        NSApp.applicationIconImage = icon
    }

    var body: some Scene {
        WindowGroup("Links") {
            LinksRootView(model: profileSession.model)
                .frame(minWidth: 900, minHeight: 600)
                .onAppear {
                    applyApplicationIcon()
                    applicationDelegate.clientModel = profileSession.model
                }
                .onChange(of: profileSession.model.profileName) { _ in
                    applicationDelegate.clientModel = profileSession.model
                }
        }
        .onChange(of: scenePhase) { phase in
            profileSession.model.scenePhaseDidChange(phase)
        }
    }
}
