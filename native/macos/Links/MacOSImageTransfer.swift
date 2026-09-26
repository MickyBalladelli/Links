import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

enum MacOSImageTransferError: Error {
    case unreadableImage
}

enum MacOSImageTransfer {
    static func rgbPixels(from imageData: Data) throws -> Data {
        guard let source = CGImageSourceCreateWithData(imageData as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil),
              image.width > 0, image.height > 0,
              image.width <= 1_600, image.height <= 1_600 else {
            throw MacOSImageTransferError.unreadableImage
        }

        let width = image.width
        let height = image.height
        var rgba = Data(count: width * height * 4)
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
        let drawn = rgba.withUnsafeMutableBytes { rawBuffer -> Bool in
            guard let baseAddress = rawBuffer.baseAddress,
                  let context = CGContext(
                    data: baseAddress,
                    width: width,
                    height: height,
                    bitsPerComponent: 8,
                    bytesPerRow: width * 4,
                    space: colorSpace,
                    bitmapInfo: bitmapInfo) else {
                return false
            }
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { throw MacOSImageTransferError.unreadableImage }

        var rgb = Data(count: width * height * 3)
        rgba.withUnsafeBytes { rgbaBytes in
            rgb.withUnsafeMutableBytes { rgbBytes in
                guard let source = rgbaBytes.bindMemory(to: UInt8.self).baseAddress,
                      let destination = rgbBytes.bindMemory(to: UInt8.self).baseAddress else {
                    return
                }
                for pixel in 0..<(width * height) {
                    destination[pixel * 3] = source[pixel * 4]
                    destination[pixel * 3 + 1] = source[pixel * 4 + 1]
                    destination[pixel * 3 + 2] = source[pixel * 4 + 2]
                }
            }
        }
        rgba.resetBytes(in: 0..<rgba.count)
        return rgb
    }

    static func nsImage(from imageData: Data) -> NSImage? {
        NSImage(data: imageData)
    }

    static func copyToClipboard(_ image: NSImage) {
        guard let pngData = pngData(from: image) else {
            showError(title: "Could not copy image", message: "Links could not convert this image.")
            return
        }

        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        guard pasteboard.setData(pngData, forType: .png) else {
            showError(title: "Could not copy image", message: "The clipboard rejected the image.")
            return
        }
    }

    private static func pngData(from image: NSImage) -> Data? {
        guard let tiffData = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffData),
              let data = bitmap.representation(using: .png, properties: [:]) else { return nil }
        return data
    }

    private static func showError(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .warning
        alert.runModal()
    }

    static func save(_ image: NSImage, onFailure: @escaping (String) -> Void) {
        guard let pngData = pngData(from: image) else {
            onFailure("Links could not convert this image.")
            return
        }

        // The right-click menu is still closing. A sheet presented during that
        // tracking loop is discarded, so wait until the menu is gone, then
        // open the dialog beside the menu.
        let anchor = NSEvent.mouseLocation
        DispatchQueue.main.async {
            NSApp.activate(ignoringOtherApps: true)
            let panel = NSSavePanel()
            panel.allowedContentTypes = [.png]
            panel.nameFieldStringValue = "Image.png"
            panel.canCreateDirectories = true
            panel.isExtensionHidden = false
            var placed = false
            let place = {
                guard panel.frame.width > 1, panel.frame.height > 1 else { return }
                guard !placed else { return }
                placed = true
                placeSavePanel(panel, near: anchor)
            }
            var token: NSObjectProtocol?
            token = NotificationCenter.default.addObserver(
                forName: NSWindow.didBecomeKeyNotification,
                object: panel,
                queue: .main
            ) { _ in
                if let token {
                    NotificationCenter.default.removeObserver(token)
                }
                place()
            }
            panel.begin { response in
                if let token {
                    NotificationCenter.default.removeObserver(token)
                }
                guard response == .OK, let url = panel.url else { return }
                let accessed = url.startAccessingSecurityScopedResource()
                defer {
                    if accessed { url.stopAccessingSecurityScopedResource() }
                }
                do {
                    try pngData.write(to: url, options: .atomic)
                } catch {
                    DispatchQueue.main.async {
                        onFailure(error.localizedDescription)
                    }
                }
            }
            DispatchQueue.main.async {
                place()
                // AppKit centers the panel when it becomes key. Move it again
                // after that layout so it stays beside the menu.
                DispatchQueue.main.async {
                    placed = false
                    place()
                }
            }
        }
    }
}

private func placeSavePanel(_ panel: NSSavePanel, near anchor: NSPoint) {
    let screen = NSScreen.screens.first { $0.frame.contains(anchor) } ?? NSScreen.main
    let visible = screen?.visibleFrame ?? NSRect(x: 0, y: 0, width: 1440, height: 900)
    var frame = panel.frame
    guard frame.width > 1, frame.height > 1 else { return }
    frame.origin.x = anchor.x + 12
    frame.origin.y = anchor.y - frame.height + 12
    if frame.maxX > visible.maxX {
        frame.origin.x = max(visible.minX + 8, anchor.x - frame.width - 12)
    }
    if frame.minX < visible.minX {
        frame.origin.x = visible.minX + 8
    }
    if frame.minY < visible.minY {
        frame.origin.y = visible.minY + 8
    }
    if frame.maxY > visible.maxY {
        frame.origin.y = visible.maxY - frame.height - 8
    }
    panel.setFrame(frame, display: true)
}
