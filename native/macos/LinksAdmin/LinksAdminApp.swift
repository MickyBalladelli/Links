import SwiftUI

@main
struct LinksAdminApp: App {
    @StateObject private var model = LinksAdminModel()

    var body: some Scene {
        WindowGroup("Links Admin") {
            LinksAdminRootView(model: model)
                .frame(minWidth: 980, minHeight: 640)
        }
    }
}
