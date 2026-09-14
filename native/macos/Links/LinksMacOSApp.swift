import SwiftUI

@main
struct LinksMacOSApp: App {
    @StateObject private var model = LinksMacOSAppModel()
    @NSApplicationDelegateAdaptor(LinksMacOSApplicationDelegate.self)
    private var applicationDelegate
    @Environment(\.scenePhase) private var scenePhase

    var body: some Scene {
        WindowGroup("Links") {
            LinksRootView(model: model)
                .frame(minWidth: 900, minHeight: 600)
                .onAppear {
                    applicationDelegate.model = model
                }
        }
        .onChange(of: scenePhase) { phase in
            model.scenePhaseDidChange(phase)
        }
    }
}
