import Foundation

public struct IOSLargeFileUploadReceipt: Equatable, Sendable {
    public let attachmentID: String
    public let ciphertextSizeBytes: UInt64
    public let ciphertextSHA256: Data

    public init(attachmentID: String, ciphertextSizeBytes: UInt64,
                ciphertextSHA256: Data) throws {
        guard IOSClient.isCanonicalUUID(attachmentID), ciphertextSizeBytes > 0,
              ciphertextSHA256.count == 32 else {
            throw IOSLargeFileError.invalidUploadReceipt
        }
        self.attachmentID = attachmentID
        self.ciphertextSizeBytes = ciphertextSizeBytes
        self.ciphertextSHA256 = ciphertextSHA256
    }

    public func matches(_ metadata: IOSLargeFileMetadata) -> Bool {
        attachmentID == metadata.attachmentID
            && ciphertextSizeBytes == metadata.ciphertextSizeBytes
            && ciphertextSHA256 == metadata.ciphertextSHA256
    }
}

/// Ciphertext-only upload/download boundary for video and arbitrary files.
/// Downloaded URLs are disposable ciphertext staging files owned by the uploader.
public protocol IOSLargeFileUploader: AnyObject {
    func upload(_ file: IOSEncryptedLargeFile, accessToken: String)
        throws -> IOSLargeFileUploadReceipt
    func download(_ metadata: IOSLargeFileMetadata, accessToken: String) throws -> URL
}

/// iOS video/file flow: transcode first, stream-encrypt second, upload opaque
/// ciphertext, then send private metadata through the connected MLS core.
public final class IOSLargeFileSession {
    private let client: IOSClient
    private let messaging: IOSDirectMessaging
    private let uploader: any IOSLargeFileUploader
    private let transfer: IOSLargeFileTransfer

    public init(client: IOSClient, messaging: IOSDirectMessaging,
                uploader: any IOSLargeFileUploader,
                transfer: IOSLargeFileTransfer = IOSLargeFileTransfer()) {
        self.client = client
        self.messaging = messaging
        self.uploader = uploader
        self.transfer = transfer
    }

    public func encrypt(source: URL, destinationDirectory: URL,
                        attachmentID: String = UUID().uuidString.lowercased(),
                        mimeType: String, width: Int? = nil, height: Int? = nil,
                        durationMs: UInt64? = nil) throws -> IOSEncryptedLargeFile {
        try transfer.encrypt(
            source: source, destinationDirectory: destinationDirectory,
            attachmentID: attachmentID, mimeType: mimeType, width: width,
            height: height, durationMs: durationMs)
    }

    public func upload(_ file: IOSEncryptedLargeFile) throws -> IOSLargeFileUploadReceipt {
        let receipt = try uploader.upload(file, accessToken: client.accessToken())
        guard receipt.matches(file.metadata) else {
            throw IOSLargeFileError.invalidUploadReceipt
        }
        return receipt
    }

    public func send(_ file: IOSEncryptedLargeFile, receipt: IOSLargeFileUploadReceipt,
                     conversationID: String, recipientUserID: String) throws {
        guard receipt.matches(file.metadata) else {
            throw IOSLargeFileError.invalidUploadReceipt
        }
        try messaging.sendLargeFile(
            conversationID: conversationID, recipientUserID: recipientUserID,
            metadata: file.metadata, receipt: receipt)
    }

    /// Decrypts into a caller-selected destination only after every chunk and
    /// the complete ciphertext digest pass. Use a temporary destination when
    /// the caller wants an additional atomic publish step.
    public func downloadAndDecrypt(_ metadata: IOSLargeFileMetadata,
                                   destination: URL) throws {
        let ciphertextURL = try uploader.download(metadata, accessToken: client.accessToken())
        try transfer.decrypt(source: ciphertextURL, destination: destination, metadata: metadata)
    }
}

public extension SharedClientCore {
    func sendLargeFile(conversationID: String, recipientUserID: String,
                       metadata: IOSLargeFileMetadata,
                       receipt: IOSLargeFileUploadReceipt,
                       transport: any IOSCoreTransport) throws {
        throw IOSLargeFileError.coreUnavailable
    }
}
