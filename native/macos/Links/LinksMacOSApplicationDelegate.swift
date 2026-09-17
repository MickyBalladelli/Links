import AppKit

@MainActor
final class LinksMacOSApplicationDelegate: NSObject, NSApplicationDelegate {
    weak var model: LinksMacOSAppModel?
    private var pendingURLs: [URL] = []

    private func applyLinksIcon() {
        guard let iconURL = Bundle.main.url(forResource: "AppIcon", withExtension: "icns"),
              let icon = NSImage(contentsOf: iconURL) else { return }

        NSApp.applicationIconImage = icon

        let iconView = NSImageView(frame: NSRect(x: 0, y: 0, width: 128, height: 128))
        iconView.image = icon
        iconView.imageScaling = .scaleProportionallyUpOrDown
        NSApp.dockTile.contentView = iconView
        NSApp.dockTile.display()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        applyLinksIcon()
        DispatchQueue.main.async { [weak self] in
            self?.applyLinksIcon()
        }
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        applyLinksIcon()
    }

    var clientModel: LinksMacOSAppModel? {
        get { model }
        set {
            model = newValue
            guard let model else { return }
            pendingURLs.forEach { model.handleIncomingURL($0) }
            pendingURLs.removeAll()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        model?.shutdownForTermination()
    }

    func application(_ application: NSApplication, open urls: [URL]) {
        guard let model else {
            pendingURLs.append(contentsOf: urls)
            return
        }
        urls.forEach { model.handleIncomingURL($0) }
    }
}
