#if os(iOS)
import ImageIO
import UIKit
import UniformTypeIdentifiers

/// Result ready for the later encrypted-image attachment pipeline.
public struct IOSResizedImage: Sendable {
    public let data: Data
    public let mimeType: String
    public let width: Int
    public let height: Int

    public init(data: Data, mimeType: String, width: Int, height: Int) {
        self.data = data
        self.mimeType = mimeType
        self.width = width
        self.height = height
    }
}

public enum IOSImageResizeError: Error {
    case invalidInput
    case unableToReadImage
    case unableToEncodeImage
    case imageTooLarge
}

/// Resize images before any encryption or upload and remove EXIF GPS metadata.
public enum IOSImageResizer {
    public static let maximumImageEdge = 1_600
    public static let maximumInputBytes = 32 * 1024 * 1024

    public static func resize(_ data: Data) throws -> IOSResizedImage {
        guard !data.isEmpty, data.count <= maximumInputBytes,
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil)
                as? [CFString: Any],
              let sourceWidth = properties[kCGImagePropertyPixelWidth] as? Int,
              let sourceHeight = properties[kCGImagePropertyPixelHeight] as? Int,
              sourceWidth > 0, sourceHeight > 0 else {
            throw IOSImageResizeError.invalidInput
        }

        let needsResize = sourceWidth > maximumImageEdge || sourceHeight > maximumImageEdge
        let hasLocationMetadata = properties[kCGImagePropertyGPSDictionary] != nil
        if !needsResize && !hasLocationMetadata {
            guard let sourceType = CGImageSourceGetType(source) as String? else {
                throw IOSImageResizeError.unableToReadImage
            }
            return IOSResizedImage(
                data: data,
                mimeType: sourceMimeType(for: sourceType),
                width: sourceWidth,
                height: sourceHeight)
        }

        let image: CGImage
        if needsResize {
            let thumbnailOptions: [CFString: Any] = [
                kCGImageSourceCreateThumbnailFromImageAlways: true,
                kCGImageSourceCreateThumbnailWithTransform: true,
                kCGImageSourceThumbnailMaxPixelSize: maximumImageEdge
            ]
            guard let thumbnail = CGImageSourceCreateThumbnailAtIndex(
                source, 0, thumbnailOptions as CFDictionary) else {
                throw IOSImageResizeError.unableToReadImage
            }
            image = thumbnail
        } else {
            guard let sourceImage = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
                throw IOSImageResizeError.unableToReadImage
            }
            image = sourceImage
        }

        let outputUTI = outputType(source: source)
        let output = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            output, outputUTI as CFString, 1, nil) else {
            throw IOSImageResizeError.unableToEncodeImage
        }
        var outputProperties = properties
        outputProperties.removeValue(forKey: kCGImagePropertyGPSDictionary)
        if needsResize {
            outputProperties.removeValue(forKey: kCGImagePropertyOrientation)
            outputProperties.removeValue(forKey: kCGImagePropertyPixelWidth)
            outputProperties.removeValue(forKey: kCGImagePropertyPixelHeight)
        }
        CGImageDestinationAddImage(destination, image, outputProperties as CFDictionary)
        guard CGImageDestinationFinalize(destination) else {
            throw IOSImageResizeError.unableToEncodeImage
        }
        let encoded = output as Data
        guard !encoded.isEmpty, encoded.count <= maximumInputBytes else {
            throw IOSImageResizeError.imageTooLarge
        }
        return IOSResizedImage(
            data: encoded,
            mimeType: mimeType(for: outputUTI),
            width: image.width,
            height: image.height)
    }

    private static func outputType(source: CGImageSource) -> String {
        guard let sourceType = CGImageSourceGetType(source) as String? else {
            return UTType.png.identifier
        }
        if sourceType == UTType.jpeg.identifier || sourceType == UTType.png.identifier
            || sourceType == UTType.heic.identifier || sourceType == UTType.heif.identifier {
            return sourceType
        }
        return UTType.png.identifier
    }

    private static func mimeType(for uti: String) -> String {
        if uti == UTType.jpeg.identifier { return "image/jpeg" }
        if uti == UTType.heic.identifier { return "image/heic" }
        if uti == UTType.heif.identifier { return "image/heif" }
        return "image/png"
    }

    private static func sourceMimeType(for uti: String) -> String {
        if uti == UTType.jpeg.identifier { return "image/jpeg" }
        if uti == UTType.png.identifier { return "image/png" }
        if uti == UTType.heic.identifier { return "image/heic" }
        if uti == UTType.heif.identifier { return "image/heif" }
        if uti == UTType.gif.identifier { return "image/gif" }
        if let webp = UTType(filenameExtension: "webp"), uti == webp.identifier {
            return "image/webp"
        }
        return "application/octet-stream"
    }
}
#endif
