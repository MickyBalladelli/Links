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

/// Resize, transcode, and remove EXIF GPS metadata before encryption or upload.
public enum IOSImageResizer {
    public static let maximumImageEdge = 1_600
    public static let maximumInputBytes = 32 * 1024 * 1024
    public static let lossyCompressionQuality = 0.8

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

        let thumbnailOptions: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maximumImageEdge
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(
            source, 0, thumbnailOptions as CFDictionary) else {
            throw IOSImageResizeError.unableToReadImage
        }

        var outputProperties = properties
        outputProperties.removeValue(forKey: kCGImagePropertyGPSDictionary)
        outputProperties.removeValue(forKey: kCGImagePropertyOrientation)
        outputProperties.removeValue(forKey: kCGImagePropertyPixelWidth)
        outputProperties.removeValue(forKey: kCGImagePropertyPixelHeight)
        outputProperties[kCGImageDestinationLossyCompressionQuality] = lossyCompressionQuality

        let outputTypes = [avifTypeIdentifier, webPTypeIdentifier]
        for outputUTI in outputTypes {
            let output = NSMutableData()
            guard let destination = CGImageDestinationCreateWithData(
                output, outputUTI as CFString, 1, nil) else {
                continue
            }
            CGImageDestinationAddImage(destination, image, outputProperties as CFDictionary)
            guard CGImageDestinationFinalize(destination) else {
                continue
            }
            let encoded = output as Data
            guard !encoded.isEmpty else {
                continue
            }
            guard encoded.count <= maximumInputBytes else {
                throw IOSImageResizeError.imageTooLarge
            }
            return IOSResizedImage(
                data: encoded,
                mimeType: mimeType(for: outputUTI),
                width: image.width,
                height: image.height)
        }

        throw IOSImageResizeError.unableToEncodeImage
    }

    private static let webPTypeIdentifier =
        UTType(filenameExtension: "webp")?.identifier ?? "org.webmproject.webp"
    private static let avifTypeIdentifier =
        UTType(filenameExtension: "avif")?.identifier ?? "public.avif"

    private static func mimeType(for uti: String) -> String {
        if uti == avifTypeIdentifier { return "image/avif" }
        return "image/webp"
    }
}
#endif
