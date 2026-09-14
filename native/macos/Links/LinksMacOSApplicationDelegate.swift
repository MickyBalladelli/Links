import AppKit

@MainActor
final class LinksMacOSApplicationDelegate: NSObject, NSApplicationDelegate {
    weak var model: LinksMacOSAppModel?

    func applicationWillTerminate(_ notification: Notification) {
        model?.shutdownForTermination()
    }
}
