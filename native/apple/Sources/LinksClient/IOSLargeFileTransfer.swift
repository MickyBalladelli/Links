import CryptoKit
import Foundation

public enum IOSLargeFileError: Error {
    case invalidInput
    case invalidMetadata
    case invalidUploadReceipt
    case encryptionFailed
    case decryptionFailed
    case integrityFailure
    case coreUnavailable
}

/// Private metadata for a chunk-encrypted MP4 or arbitrary large file.
public struct IOSLargeFileMetadata: Sendable {
    public let attachmentID: String
    public let mimeType: String
    public let originalSizeBytes: UInt64
    public let ciphertextSizeBytes: UInt64
    public let contentKey: Data
    public let nonce: Data
    public let ciphertextSHA256: Data
    public let width: Int?
    public let height: Int?
    public let durationMs: UInt64?

    public init(attachmentID: String, mimeType: String, originalSizeBytes: UInt64,
                ciphertextSizeBytes: UInt64, contentKey: Data, nonce: Data,
                ciphertextSHA256: Data, width: Int? = nil, height: Int? = nil,
                durationMs: UInt64? = nil) throws {
        guard Self.isUUID(attachmentID),
              mimeType == "video/mp4" || mimeType == "application/octet-stream",
              originalSizeBytes > 0, ciphertextSizeBytes > 0,
              contentKey.count == 32, nonce.count == 12,
              ciphertextSHA256.count == 32,
              width.map({ $0 > 0 }) ?? true,
              height.map({ $0 > 0 }) ?? true,
              durationMs.map({ $0 > 0 }) ?? true else {
            throw IOSLargeFileError.invalidMetadata
        }
        let isVideo = mimeType == "video/mp4"
        guard isVideo == (width != nil && height != nil && durationMs != nil) else {
            throw IOSLargeFileError.invalidMetadata
        }
        self.attachmentID = attachmentID
        self.mimeType = mimeType
        self.originalSizeBytes = originalSizeBytes
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.contentKey = contentKey
        self.nonce = nonce
        self.ciphertextSHA256 = ciphertextSHA256
        self.width = width
        self.height = height
        self.durationMs = durationMs
    }

    private static func isUUID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value) else { return false }
        return uuid.uuidString.lowercased() == value
    }
}

public struct IOSEncryptedLargeFile: Sendable {
    public let metadata: IOSLargeFileMetadata
    public let ciphertextURL: URL
}

/// Streams the same ChaCha20-Poly1305 chunk format as links-client-core.
/// The source is read in bounded chunks and the output is a local ciphertext
/// staging file suitable for a WebRTC DataChannel source.
public final class IOSLargeFileTransfer {
    public static let ciphertextChunkBytes = 256 * 1024
    public static let plaintextChunkBytes = ciphertextChunkBytes - 16
    private static let aadPrefix = Data("links/large-file/attachment/v1\0".utf8)

    public init() {}

    public func encrypt(source: URL, destinationDirectory: URL,
                        attachmentID: String = UUID().uuidString.lowercased(),
                        mimeType: String, width: Int? = nil, height: Int? = nil,
                        durationMs: UInt64? = nil) throws -> IOSEncryptedLargeFile {
        guard FileManager.default.fileExists(atPath: source.path),
              let sourceAttributes = try? FileManager.default.attributesOfItem(atPath: source.path),
              let sourceSize = (sourceAttributes[.size] as? NSNumber)?.uint64Value,
              sourceSize > 0 else {
            throw IOSLargeFileError.invalidInput
        }
        let key = Self.randomBytes(count: 32)
        let nonce = Self.randomBytes(count: 12)
        let outputURL = destinationDirectory.appendingPathComponent(
            ".links-encrypted-\(UUID().uuidString.lowercased()).blob")
        guard !FileManager.default.fileExists(atPath: outputURL.path) else {
            throw IOSLargeFileError.invalidInput
        }
        do {
            guard FileManager.default.createFile(atPath: outputURL.path, contents: nil) else {
                throw IOSLargeFileError.encryptionFailed
            }
            let input = try FileHandle(forReadingFrom: source)
            let output = try FileHandle(forWritingTo: outputURL)
            defer {
                try? input.close()
                try? output.close()
            }
            var hash = SHA256()
            var chunkIndex: UInt64 = 0
            var ciphertextSize: UInt64 = 0
            var plaintextSize: UInt64 = 0
            while let plaintext = try Self.readChunk(input, capacity: Self.plaintextChunkBytes) {
                let ciphertext = try Self.seal(
                    plaintext, key: key, nonce: nonce,
                    attachmentID: attachmentID, chunkIndex: chunkIndex)
                try output.write(contentsOf: ciphertext)
                hash.update(data: ciphertext)
                plaintextSize = try Self.adding(plaintextSize, UInt64(plaintext.count))
                ciphertextSize = try Self.adding(ciphertextSize, UInt64(ciphertext.count))
                chunkIndex = try Self.adding(chunkIndex, 1)
            }
            guard plaintextSize == sourceSize else {
                throw IOSLargeFileError.encryptionFailed
            }
            try output.synchronize()
            let metadata = try IOSLargeFileMetadata(
                attachmentID: attachmentID, mimeType: mimeType,
                originalSizeBytes: sourceSize, ciphertextSizeBytes: ciphertextSize,
                contentKey: key, nonce: nonce,
                ciphertextSHA256: Data(hash.finalize()), width: width,
                height: height, durationMs: durationMs)
            return IOSEncryptedLargeFile(metadata: metadata, ciphertextURL: outputURL)
        } catch {
            try? FileManager.default.removeItem(at: outputURL)
            if error is IOSLargeFileError { throw error }
            throw IOSLargeFileError.encryptionFailed
        }
    }

    public func decrypt(source: URL, destination: URL,
                        metadata: IOSLargeFileMetadata) throws {
        try validate(metadata)
        guard FileManager.default.fileExists(atPath: source.path),
              let sourceAttributes = try? FileManager.default.attributesOfItem(atPath: source.path),
              let sourceSize = (sourceAttributes[.size] as? NSNumber)?.uint64Value,
              sourceSize == metadata.ciphertextSizeBytes,
              !FileManager.default.fileExists(atPath: destination.path) else {
            throw IOSLargeFileError.invalidInput
        }
        do {
            FileManager.default.createFile(atPath: destination.path, contents: nil)
            let input = try FileHandle(forReadingFrom: source)
            let output = try FileHandle(forWritingTo: destination)
            defer {
                try? input.close()
                try? output.close()
            }
            let chunkCount = try Self.chunkCount(metadata.originalSizeBytes)
            var hash = SHA256()
            var ciphertextOffset: UInt64 = 0
            for chunkIndex in 0..<chunkCount {
                let chunkStart = try Self.multiplying(
                    chunkIndex, UInt64(Self.plaintextChunkBytes))
                let remaining = metadata.originalSizeBytes - chunkStart
                let plaintextSize = Int(min(remaining, UInt64(Self.plaintextChunkBytes)))
                let ciphertextSize = plaintextSize + 16
                guard let ciphertext = try input.read(upToCount: ciphertextSize),
                      ciphertext.count == ciphertextSize else {
                    throw IOSLargeFileError.integrityFailure
                }
                hash.update(data: ciphertext)
                let plaintext = try Self.open(
                    ciphertext, key: metadata.contentKey, nonce: metadata.nonce,
                    attachmentID: metadata.attachmentID, chunkIndex: chunkIndex)
                try output.write(contentsOf: plaintext)
                ciphertextOffset = try Self.adding(ciphertextOffset, UInt64(ciphertextSize))
            }
            guard ciphertextOffset == metadata.ciphertextSizeBytes,
                  Data(hash.finalize()) == metadata.ciphertextSHA256,
                  (try input.read(upToCount: 1))?.isEmpty != false else {
                throw IOSLargeFileError.integrityFailure
            }
            try output.synchronize()
        } catch {
            try? FileManager.default.removeItem(at: destination)
            if error is IOSLargeFileError { throw error }
            throw IOSLargeFileError.decryptionFailed
        }
    }

    private func validate(_ metadata: IOSLargeFileMetadata) throws {
        _ = try IOSLargeFileMetadata(
            attachmentID: metadata.attachmentID, mimeType: metadata.mimeType,
            originalSizeBytes: metadata.originalSizeBytes,
            ciphertextSizeBytes: metadata.ciphertextSizeBytes,
            contentKey: metadata.contentKey, nonce: metadata.nonce,
            ciphertextSHA256: metadata.ciphertextSHA256, width: metadata.width,
            height: metadata.height, durationMs: metadata.durationMs)
        let chunkCount = try Self.chunkCount(metadata.originalSizeBytes)
        let expected = try Self.adding(
            metadata.originalSizeBytes,
            try Self.multiplying(chunkCount, 16))
        guard expected == metadata.ciphertextSizeBytes else {
            throw IOSLargeFileError.invalidMetadata
        }
    }

    private static func seal(_ plaintext: Data, key: Data, nonce: Data,
                             attachmentID: String, chunkIndex: UInt64) throws -> Data {
        let sealed = try ChaChaPoly.seal(
            plaintext, using: SymmetricKey(data: key),
            nonce: ChaChaPoly.Nonce(data: chunkNonce(nonce, chunkIndex)),
            authenticating: chunkAAD(attachmentID, chunkIndex))
        return sealed.combined
    }

    private static func open(_ ciphertext: Data, key: Data, nonce: Data,
                             attachmentID: String, chunkIndex: UInt64) throws -> Data {
        let box = try ChaChaPoly.SealedBox(combined: ciphertext)
        return try ChaChaPoly.open(
            box, using: SymmetricKey(data: key),
            authenticating: chunkAAD(attachmentID, chunkIndex))
    }

    private static func chunkNonce(_ base: Data, _ chunkIndex: UInt64) -> Data {
        var nonce = base
        var value = chunkIndex.bigEndian
        withUnsafeBytes(of: &value) { bytes in
            for index in bytes.indices {
                nonce[4 + index] ^= bytes[index]
            }
        }
        return nonce
    }

    private static func chunkAAD(_ attachmentID: String, _ chunkIndex: UInt64) -> Data {
        var aad = aadPrefix
        aad.append(contentsOf: attachmentID.utf8)
        var value = chunkIndex.bigEndian
        withUnsafeBytes(of: &value) { aad.append(contentsOf: $0) }
        return aad
    }

    private static func chunkCount(_ size: UInt64) throws -> UInt64 {
        guard size > 0 else { throw IOSLargeFileError.invalidMetadata }
        return try adding(size, UInt64(plaintextChunkBytes - 1))
            / UInt64(plaintextChunkBytes)
    }

    private static func adding(_ left: UInt64, _ right: UInt64) throws -> UInt64 {
        let (value, overflow) = left.addingReportingOverflow(right)
        if overflow { throw IOSLargeFileError.integrityFailure }
        return value
    }

    private static func multiplying(_ left: UInt64, _ right: UInt64) throws -> UInt64 {
        let (value, overflow) = left.multipliedReportingOverflow(by: right)
        if overflow { throw IOSLargeFileError.integrityFailure }
        return value
    }

    private static func randomBytes(count: Int) -> Data {
        var generator = SystemRandomNumberGenerator()
        return Data((0..<count).map { _ in
            UInt8.random(in: UInt8.min...UInt8.max, using: &generator)
        })
    }

    private static func readChunk(_ input: FileHandle, capacity: Int) throws -> Data? {
        var chunk = Data()
        chunk.reserveCapacity(capacity)
        while chunk.count < capacity {
            guard let part = try input.read(upToCount: capacity - chunk.count),
                  !part.isEmpty else {
                break
            }
            chunk.append(part)
        }
        return chunk.isEmpty ? nil : chunk
    }
}
