import SwiftUI

@main
struct LinksIOSApp: App {
    @StateObject private var model = IOSMobileAppModel()

    var body: some Scene {
        WindowGroup {
            IOSMobileRootView(model: model)
                .onOpenURL { url in
                    model.handleIncomingURL(url)
                }
        }
    }
}
