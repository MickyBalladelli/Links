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
        let pasteboard = NSPasteboard.general
        pasteboard.clearContents()
        pasteboard.writeObjects([image])
    }

    static func save(_ image: NSImage) {
        guard let tiffData = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffData),
              let pngData = bitmap.representation(using: .png, properties: [:]) else {
            return
        }

        let panel = NSSavePanel()
        panel.allowedContentTypes = [.png]
        panel.nameFieldStringValue = "Image.png"
        panel.canCreateDirectories = true
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                try pngData.write(to: url, options: .atomic)
            } catch {
                NSAlert(error: error).runModal()
            }
        }
    }
}
