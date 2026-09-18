import AppKit
import SwiftUI

private final class LinksAdminApplicationDelegate: NSObject, NSApplicationDelegate {
    func applicationWillFinishLaunching(_ notification: Notification) {
        let iconURL = Bundle.main.url(
            forResource: "AdminAppIcon",
            withExtension: "icns")
            ?? Bundle.main.url(forResource: "icon-admin", withExtension: "png")
        guard let iconURL, let icon = NSImage(contentsOf: iconURL) else { return }
        NSApplication.shared.applicationIconImage = icon
    }
}

@main
struct LinksAdminApp: App {
    @NSApplicationDelegateAdaptor(LinksAdminApplicationDelegate.self)
    private var applicationDelegate
    @StateObject private var model = LinksAdminModel()

    var body: some Scene {
        WindowGroup("Links Admin") {
            LinksAdminRootView(model: model)
                .frame(minWidth: 980, minHeight: 640)
        }
    }
}
