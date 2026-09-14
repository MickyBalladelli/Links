import AppKit

@MainActor
final class LinksMacOSApplicationDelegate: NSObject, NSApplicationDelegate {
    weak var model: LinksMacOSAppModel?
    private var pendingURLs: [URL] = []

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
