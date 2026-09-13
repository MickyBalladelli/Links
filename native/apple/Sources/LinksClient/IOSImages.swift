#if os(iOS)
import CryptoKit
import Foundation
import ImageIO
import UIKit

public struct IOSImageMetadata: Sendable {
    public let attachmentID: String
    public let mimeType: String
    public let ciphertextSizeBytes: UInt64
    public let contentKey: Data
    public let nonce: Data
    public let ciphertextSHA256: Data
    public let width: Int
    public let height: Int
    public let blurHash: String

    public init(attachmentID: String, mimeType: String, ciphertextSizeBytes: UInt64,
                contentKey: Data, nonce: Data, ciphertextSHA256: Data,
                width: Int, height: Int, blurHash: String) throws {
        guard IOSClient.isCanonicalUUID(attachmentID),
              mimeType == "image/webp" || mimeType == "image/avif",
              (17...32 * 1024 * 1024 + 16).contains(ciphertextSizeBytes),
              contentKey.count == 32, nonce.count == 12,
              ciphertextSHA256.count == 32,
              (1...IOSImageResizer.maximumImageEdge).contains(width),
              (1...IOSImageResizer.maximumImageEdge).contains(height),
              Self.isBlurHash(blurHash) else {
            throw IOSImageError.invalidMetadata
        }
        self.attachmentID = attachmentID
        self.mimeType = mimeType
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.contentKey = contentKey
        self.nonce = nonce
        self.ciphertextSHA256 = ciphertextSHA256
        self.width = width
        self.height = height
        self.blurHash = blurHash
    }

    private static func isBlurHash(_ value: String) -> Bool {
        let alphabet = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~"
        return value.count == 28 && value.first == "L"
            && value.allSatisfy { alphabet.contains($0) }
    }
}

public struct IOSEncryptedImage: Sendable {
    public let metadata: IOSImageMetadata
    public let ciphertext: Data

    public init(metadata: IOSImageMetadata, ciphertext: Data) throws {
        guard ciphertext.count == Int(metadata.ciphertextSizeBytes),
              Data(SHA256.hash(data: ciphertext)) == metadata.ciphertextSHA256 else {
            throw IOSImageError.integrityFailure
        }
        self.metadata = metadata
        self.ciphertext = ciphertext
    }
}

public struct IOSImageUploadReceipt: Equatable, Sendable {
    public let attachmentID: String
    public let ciphertextSizeBytes: UInt64
    public let ciphertextSHA256: Data

    public init(attachmentID: String, ciphertextSizeBytes: UInt64,
                ciphertextSHA256: Data) throws {
        guard IOSClient.isCanonicalUUID(attachmentID),
              (17...32 * 1024 * 1024 + 16).contains(ciphertextSizeBytes),
              ciphertextSHA256.count == 32 else {
            throw IOSImageError.invalidUploadReceipt
        }
        self.attachmentID = attachmentID
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.ciphertextSHA256 = ciphertextSHA256
    }

    public func matches(_ metadata: IOSImageMetadata) -> Bool {
        attachmentID == metadata.attachmentID
            && ciphertextSizeBytes == metadata.ciphertextSizeBytes
            && ciphertextSHA256 == metadata.ciphertextSHA256
    }
}

public protocol IOSImageUploader: AnyObject {
    /// Upload ciphertext only over authenticated TLS.
    func upload(_ image: IOSEncryptedImage, accessToken: String)
        throws -> IOSImageUploadReceipt
    /// Return the opaque ciphertext for this private attachment metadata.
    func download(_ metadata: IOSImageMetadata, accessToken: String) throws -> Data
}

public protocol IOSImageRenderer: AnyObject {
    /// Renderer owns the image after this callback returns.
    func render(_ image: UIImage, blurHash: String) throws
}

/// Encrypted-on-disk image cache. It never stores plaintext pixels or keys.
public final class IOSImageCache {
    private let directory: URL
    private let lock = NSLock()

    public init(directory: URL = FileManager.default.urls(
        for: .cachesDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("links-images-v1", isDirectory: true)) throws {
        self.directory = directory
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true)
    }

    public func read(_ metadata: IOSImageMetadata) throws -> Data? {
        lock.lock()
        defer { lock.unlock() }
        let url = try fileURL(for: metadata.attachmentID)
        guard FileManager.default.fileExists(atPath: url.path) else { return nil }
        let ciphertext = try Data(contentsOf: url, options: [.mappedIfSafe])
        guard isValid(ciphertext, metadata: metadata) else {
            try? FileManager.default.removeItem(at: url)
            return nil
        }
        return ciphertext
    }

    public func write(_ image: IOSEncryptedImage) throws {
        lock.lock()
        defer { lock.unlock() }
        let url = try fileURL(for: image.metadata.attachmentID)
        try image.ciphertext.write(to: url, options: [.atomic])
    }

    public func remove(_ attachmentID: String) throws {
        lock.lock()
        defer { lock.unlock() }
        let url = try fileURL(for: attachmentID)
        try? FileManager.default.removeItem(at: url)
    }

    private func fileURL(for attachmentID: String) throws -> URL {
        guard IOSClient.isCanonicalUUID(attachmentID) else {
            throw IOSImageError.invalidMetadata
        }
        return directory.appendingPathComponent(attachmentID).appendingPathExtension("blob")
    }

    private func isValid(_ ciphertext: Data, metadata: IOSImageMetadata) -> Bool {
        ciphertext.count == Int(metadata.ciphertextSizeBytes)
            && Data(SHA256.hash(data: ciphertext)) == metadata.ciphertextSHA256
    }
}

public final class IOSImageSession {
    private let messaging: IOSDirectMessaging
    private let client: IOSClient
    private let uploader: IOSImageUploader
    private let cache: IOSImageCache

    public init(messaging: IOSDirectMessaging, client: IOSClient,
                uploader: IOSImageUploader, cache: IOSImageCache = try IOSImageCache())
        throws {
        self.messaging = messaging
        self.client = client
        self.uploader = uploader
        self.cache = cache
    }

    /// Normalize, create a private BlurHash, then encrypt before upload.
    public func prepareAndEncrypt(_ source: Data) throws -> IOSEncryptedImage {
        let normalized = try IOSImageResizer.resize(source)
        let pixels = try Self.rgbPixels(for: normalized.data)
        let blurHash = try messaging.encodeImageBlurHash(
            rgbPixels: pixels.data, width: pixels.width, height: pixels.height)
        return try messaging.encryptImage(
            normalized.data,
            attachmentID: UUID().uuidString.lowercased(),
            mimeType: normalized.mimeType,
            width: normalized.width,
            height: normalized.height,
            blurHash: blurHash)
    }

    public func upload(_ image: IOSEncryptedImage) throws -> IOSImageUploadReceipt {
        let receipt = try uploader.upload(image, accessToken: client.accessToken())
        guard receipt.matches(image.metadata) else {
            throw IOSImageError.invalidUploadReceipt
        }
        return receipt
    }

    public func send(conversationID: String, recipientUserID: String,
                     image: IOSEncryptedImage, receipt: IOSImageUploadReceipt) throws {
        guard receipt.matches(image.metadata) else {
            throw IOSImageError.invalidUploadReceipt
        }
        try messaging.sendImage(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: image.metadata, receipt: receipt)
    }

    /// Load cached ciphertext or fetch it, decrypt it, and render verified pixels.
    public func downloadAndRender(_ metadata: IOSImageMetadata,
                                  renderer: IOSImageRenderer) throws {
        var ciphertext = try cache.read(metadata)
        if ciphertext == nil {
            let downloaded = try uploader.download(
                metadata, accessToken: client.accessToken())
            let image = try IOSEncryptedImage(metadata: metadata, ciphertext: downloaded)
            try cache.write(image)
            ciphertext = downloaded
        }
        guard var ciphertext else { throw IOSImageError.integrityFailure }
        var plaintext = try messaging.decryptImage(metadata, ciphertext: ciphertext)
        defer {
            ciphertext.resetBytes(in: 0..<ciphertext.count)
            plaintext.resetBytes(in: 0..<plaintext.count)
        }
        guard let image = UIImage(data: plaintext) else {
            throw IOSImageError.unableToRender
        }
        try renderer.render(image, blurHash: metadata.blurHash)
    }

    private static func rgbPixels(for data: Data) throws -> (data: Data, width: Int, height: Int) {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw IOSImageError.unableToReadImage
        }
        let width = image.width
        let height = image.height
        guard width > 0, height > 0,
              width <= IOSImageResizer.maximumImageEdge,
              height <= IOSImageResizer.maximumImageEdge else {
            throw IOSImageError.invalidMetadata
        }
        var rgba = Data(count: width * height * 4)
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        let bitmapInfo = CGImageAlphaInfo.premultipliedLast.rawValue
        let drawn = rgba.withUnsafeMutableBytes { rawBuffer -> Bool in
            guard let baseAddress = rawBuffer.baseAddress,
                  let context = CGContext(
                    data: baseAddress, width: width, height: height,
                    bitsPerComponent: 8, bytesPerRow: width * 4,
                    space: colorSpace, bitmapInfo: bitmapInfo) else {
                return false
            }
            context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard drawn else { throw IOSImageError.unableToReadImage }
        var rgb = Data(count: width * height * 3)
        rgb.withUnsafeMutableBytes { output in
            rgba.withUnsafeBytes { input in
                guard let outputBase = output.baseAddress,
                      let inputBase = input.baseAddress else { return }
                let inputBytes = inputBase.assumingMemoryBound(to: UInt8.self)
                let outputBytes = outputBase.assumingMemoryBound(to: UInt8.self)
                for index in 0..<(width * height) {
                    outputBytes[index * 3] = inputBytes[index * 4]
                    outputBytes[index * 3 + 1] = inputBytes[index * 4 + 1]
                    outputBytes[index * 3 + 2] = inputBytes[index * 4 + 2]
                }
            }
        }
        rgba.resetBytes(in: 0..<rgba.count)
        return (rgb, width, height)
    }
}

public enum IOSImageError: Error {
    case invalidMetadata
    case invalidUploadReceipt
    case integrityFailure
    case unableToReadImage
    case unableToRender
    case coreUnavailable
}

public extension SharedClientCore {
    func encodeImageBlurHash(rgbPixels: Data, width: Int, height: Int) throws -> String {
        throw IOSImageError.coreUnavailable
    }

    func encryptImage(_ image: Data, attachmentID: String, mimeType: String,
                      width: Int, height: Int, blurHash: String) throws -> IOSEncryptedImage {
        throw IOSImageError.coreUnavailable
    }

    func decryptImage(_ metadata: IOSImageMetadata, ciphertext: Data) throws -> Data {
        throw IOSImageError.coreUnavailable
    }

    func sendImage(conversationID: String, recipientUserID: String,
                   metadata: IOSImageMetadata, receipt: IOSImageUploadReceipt,
                   transport: any IOSCoreTransport) throws {
        throw IOSImageError.coreUnavailable
    }
}
#endif
