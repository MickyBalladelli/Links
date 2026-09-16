import SwiftUI

@main
struct LinksMacOSApp: App {
    @StateObject private var profileSession = LinksMacOSProfileSession()
    @NSApplicationDelegateAdaptor(LinksMacOSApplicationDelegate.self)
    private var applicationDelegate
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup("Links") {
            LinksRootView(model: profileSession.model)
                .frame(minWidth: 900, minHeight: 600)
                .onAppear {
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
