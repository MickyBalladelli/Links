import Foundation
import CLinksDesktopClient
import LinksClient
import LinksKeyStore

private final class IOSRustCoreCallbackBox: @unchecked Sendable {
    let signer: any SharedCoreIdentitySigner
    let secrets: IOSKeychainSecretProvider
    let stateStore: IOSEncryptedStateStore
    var transport: (any IOSCoreTransport)?
    var onText: ((IOSReceivedTextMessage) -> Void)?

    init(signer: any SharedCoreIdentitySigner,
         secrets: IOSKeychainSecretProvider,
         stateStore: IOSEncryptedStateStore) {
        self.signer = signer
        self.secrets = secrets
        self.stateStore = stateStore
    }
}

private func callbackBox(_ pointer: UnsafeMutableRawPointer?) -> IOSRustCoreCallbackBox? {
    guard let pointer else { return nil }
    return Unmanaged<IOSRustCoreCallbackBox>.fromOpaque(pointer).takeUnretainedValue()
}

private func callbackData(_ pointer: UnsafePointer<UInt8>?, _ length: Int) -> Data? {
    guard length >= 0, (length == 0 || pointer != nil) else { return nil }
    guard let pointer else { return Data() }
    return Data(bytes: pointer, count: length)
}

private func callbackKey(_ pointer: UnsafePointer<UInt8>?, _ length: Int) -> String? {
    guard let data = callbackData(pointer, length),
          let key = String(data: data, encoding: .utf8) else { return nil }
    return key
}

// Callback failures are local provider failures, not bearer-token expiry.
private func callbackStatus(_: Error) -> Int32 {
    return Int32(LINKS_DESKTOP_PROVIDER)
}

private func iosRustSign(
    _ context: UnsafeMutableRawPointer?,
    _ bytes: UnsafePointer<UInt8>?,
    _ length: Int,
    _ output: UnsafeMutablePointer<UInt8>?) -> Int32 {
    guard let box = callbackBox(context),
          let bytes = callbackData(bytes, length),
          let output else { return Int32(LINKS_DESKTOP_INVALID) }
    do {
        let signature = try box.signer.sign(bytes)
        guard signature.count == 64 else { return Int32(LINKS_DESKTOP_AUTHENTICATION) }
        signature.withUnsafeBytes { raw in
            output.initialize(from: raw.bindMemory(to: UInt8.self).baseAddress!, count: 64)
        }
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        return callbackStatus(error)
    }
}

private func iosRustStoreSecret(
    _ context: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<UInt8>?,
    _ keyLength: Int,
    _ secret: UnsafePointer<UInt8>?,
    _ secretLength: Int) -> Int32 {
    guard let box = callbackBox(context),
          let key = callbackKey(key, keyLength),
          let secret = callbackData(secret, secretLength) else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    do {
        try box.secrets.store(secret, for: key)
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        return callbackStatus(error)
    }
}

private func iosRustLoadSecret(
    _ context: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<UInt8>?,
    _ keyLength: Int,
    _ output: UnsafeMutablePointer<UInt8>?,
    _ capacity: Int,
    _ outputLength: UnsafeMutablePointer<Int>?) -> Int32 {
    guard let box = callbackBox(context),
          let key = callbackKey(key, keyLength),
          let outputLength else { return Int32(LINKS_DESKTOP_INVALID) }
    do {
        let secret = try box.secrets.load(for: key)
        outputLength.pointee = secret.count
        guard let output, capacity >= secret.count else {
            return Int32(LINKS_DESKTOP_INVALID)
        }
        secret.withUnsafeBytes { raw in
            output.initialize(from: raw.bindMemory(to: UInt8.self).baseAddress!, count: secret.count)
        }
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        outputLength.pointee = 0
        return callbackStatus(error)
    }
}

private func iosRustDeleteSecret(
    _ context: UnsafeMutableRawPointer?,
    _ key: UnsafePointer<UInt8>?,
    _ keyLength: Int) -> Int32 {
    guard let box = callbackBox(context), let key = callbackKey(key, keyLength) else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    do {
        try box.secrets.delete(key)
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        return callbackStatus(error)
    }
}

private func iosRustLoadState(
    _ context: UnsafeMutableRawPointer?,
    _ output: UnsafeMutablePointer<UInt8>?,
    _ capacity: Int,
    _ outputLength: UnsafeMutablePointer<Int>?) -> Int32 {
    guard let box = callbackBox(context), let outputLength else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    do {
        guard let state = try box.stateStore.read() else {
            outputLength.pointee = 0
            return Int32(LINKS_DESKTOP_OK)
        }
        outputLength.pointee = state.count
        // The Rust bridge probes the required size before supplying a buffer.
        // A nil output is valid for that first call.
        guard output == nil || capacity >= state.count else {
            return Int32(LINKS_DESKTOP_INVALID)
        }
        guard let output else { return Int32(LINKS_DESKTOP_OK) }
        state.withUnsafeBytes { raw in
            output.initialize(from: raw.bindMemory(to: UInt8.self).baseAddress!, count: state.count)
        }
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        outputLength.pointee = 0
        return Int32(LINKS_DESKTOP_PROVIDER)
    }
}

private func iosRustSaveState(
    _ context: UnsafeMutableRawPointer?,
    _ bytes: UnsafePointer<UInt8>?,
    _ length: Int) -> Int32 {
    guard let box = callbackBox(context), let bytes = callbackData(bytes, length) else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    do {
        try box.stateStore.write(bytes)
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        return Int32(LINKS_DESKTOP_PROVIDER)
    }
}

private func iosRustSendFrame(
    _ context: UnsafeMutableRawPointer?,
    _ bytes: UnsafePointer<UInt8>?,
    _ length: Int) -> Int32 {
    guard let box = callbackBox(context), let bytes = callbackData(bytes, length),
          let transport = box.transport else { return Int32(LINKS_DESKTOP_PROVIDER) }
    return transport.send(bytes) ? Int32(LINKS_DESKTOP_OK) : Int32(LINKS_DESKTOP_PROVIDER)
}

private func iosRustText(
    _ context: UnsafeMutableRawPointer?,
    _ conversation: UnsafePointer<UInt8>?,
    _ conversationLength: Int,
    _ senderUser: UnsafePointer<UInt8>?,
    _ senderUserLength: Int,
    _ sender: UnsafePointer<UInt8>?,
    _ senderLength: Int,
    _ text: UnsafePointer<UInt8>?,
    _ textLength: Int,
    _ sequenceID: UInt64,
    _ sentAtMs: UInt64) -> Int32 {
    guard let box = callbackBox(context),
          let conversation = callbackData(conversation, conversationLength),
          let senderUser = callbackData(senderUser, senderUserLength),
          let sender = callbackData(sender, senderLength),
          let text = callbackData(text, textLength),
          let conversationID = String(data: conversation, encoding: .utf8),
          let senderUserID = String(data: senderUser, encoding: .utf8),
          let senderDeviceID = String(data: sender, encoding: .utf8),
          let text = String(data: text, encoding: .utf8) else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    do {
        let message = try IOSReceivedTextMessage(
            conversationID: conversationID,
            senderUserID: senderUserID,
            senderDeviceID: senderDeviceID,
            text: text,
            sequenceID: sequenceID,
            sentAtMs: sentAtMs)
        box.onText?(message)
        return Int32(LINKS_DESKTOP_OK)
    } catch {
        return Int32(LINKS_DESKTOP_INVALID)
    }
}

enum IOSRustCoreError: Error {
    case status(Int32)
}

private func coreError(for status: Int32) -> Error {
    IOSRustCoreError.status(status)
}

private final class IOSRustSharedCore: SharedClientCore {
    let userID: String
    let deviceID: String
    private let pointer: OpaquePointer
    private let callbacks: IOSRustCoreCallbackBox
    private let lock = NSLock()
    private var currentIssue: IOSMessagingIssue?

    init(identity: SharedCoreIdentity,
         signer: any SharedCoreIdentitySigner,
         stateStore: IOSEncryptedStateStore,
         secrets: IOSKeychainSecretProvider) throws {
        guard let credential = identity.mlsCredential else {
            throw IOSClientError.metadataUnavailable
        }
        let publicKey = Array(identity.identity.publicKey)
        guard publicKey.count == 32 else { throw IOSClientError.metadataUnavailable }
        let publicKeyTuple: (UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
                             UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
                             UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8,
                             UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8, UInt8) = (
            publicKey[0], publicKey[1], publicKey[2], publicKey[3],
            publicKey[4], publicKey[5], publicKey[6], publicKey[7],
            publicKey[8], publicKey[9], publicKey[10], publicKey[11],
            publicKey[12], publicKey[13], publicKey[14], publicKey[15],
            publicKey[16], publicKey[17], publicKey[18], publicKey[19],
            publicKey[20], publicKey[21], publicKey[22], publicKey[23],
            publicKey[24], publicKey[25], publicKey[26], publicKey[27],
            publicKey[28], publicKey[29], publicKey[30], publicKey[31])
        let callbacks = IOSRustCoreCallbackBox(
            signer: signer, secrets: secrets, stateStore: stateStore)
        var callbackTable = LinksDesktopCoreCallbacks(
            abi_version: 1,
            context: Unmanaged.passUnretained(callbacks).toOpaque(),
            sign: iosRustSign,
            store_secret: iosRustStoreSecret,
            load_secret: iosRustLoadSecret,
            delete_secret: iosRustDeleteSecret,
            load_state: iosRustLoadState,
            save_state: iosRustSaveState,
            send_frame: iosRustSendFrame,
            on_text: iosRustText,
            identity_public_key: publicKeyTuple)
        var created: OpaquePointer?
        let status = credential.withUnsafeBytes { credentialBytes in
            identity.userID.withCString { userBytes in
                identity.deviceID.withCString { deviceBytes in
                    withUnsafeMutablePointer(to: &callbackTable) { callbackPointer in
                        links_desktop_core_create(
                            UnsafeRawPointer(userBytes).assumingMemoryBound(to: UInt8.self),
                            identity.userID.utf8.count,
                            UnsafeRawPointer(deviceBytes).assumingMemoryBound(to: UInt8.self),
                            identity.deviceID.utf8.count,
                            credentialBytes.bindMemory(to: UInt8.self).baseAddress!,
                            credential.count,
                            callbackPointer,
                            &created)
                    }
                }
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK), let created else {
            throw coreError(for: status)
        }
        userID = identity.userID
        deviceID = identity.deviceID
        pointer = created
        self.callbacks = callbacks
    }

    deinit {
        links_desktop_core_destroy(pointer)
    }

    var messagingIssue: IOSMessagingIssue? {
        lock.lock()
        defer { lock.unlock() }
        return currentIssue
    }

    var pendingOutboxCount: Int {
        lock.lock()
        defer { lock.unlock() }
        var count = 0
        _ = links_desktop_core_pending_outbox_count(pointer, &count)
        return count
    }

    var pendingRetryCount: Int {
        lock.lock()
        defer { lock.unlock() }
        var count = 0
        _ = links_desktop_core_pending_retry_count(pointer, &count)
        return count
    }

    func durableCursor() throws -> UInt64 {
        lock.lock()
        defer { lock.unlock() }
        var cursor: UInt64 = 0
        let status = links_desktop_core_durable_cursor(pointer, &cursor)
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        return cursor
    }

    func createHello(accessToken: String, lastSeenCursor: UInt64) throws -> Data {
        lock.lock()
        defer { lock.unlock() }
        var buffer = Data(count: 1024 * 1024)
        let capacity = buffer.count
        var length = 0
        let status = buffer.withUnsafeMutableBytes { output in
            accessToken.withCString { token in
                links_desktop_core_create_hello(
                    pointer,
                    UnsafeRawPointer(token).assumingMemoryBound(to: UInt8.self),
                    accessToken.utf8.count,
                    lastSeenCursor,
                    output.bindMemory(to: UInt8.self).baseAddress!,
                    capacity,
                    &length)
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK) else {
            switch status {
            case Int32(LINKS_DESKTOP_STALE_CURSOR):
                currentIssue = .staleCursor
            case Int32(LINKS_DESKTOP_AUTHENTICATION):
                currentIssue = .authenticationExpired
            default:
                currentIssue = .dependencyOutage
            }
            throw coreError(for: status)
        }
        buffer.removeSubrange(length..<buffer.count)
        return buffer
    }

    func handleServerFrame(_ frame: Data, transport: any IOSCoreTransport,
                           fullSync: Bool,
                           onTextMessage: (IOSReceivedTextMessage) -> Void)
        throws -> IOSCoreFrameResult {
        return try withoutActuallyEscaping(onTextMessage) { escapableMessageHandler in
            lock.lock()
            callbacks.transport = transport
            callbacks.onText = escapableMessageHandler
            let status = frame.withUnsafeBytes { bytes in
                links_desktop_core_handle_server_frame(
                    pointer,
                    bytes.bindMemory(to: UInt8.self).baseAddress!,
                    frame.count)
            }
            callbacks.transport = nil
            callbacks.onText = nil
            if status != Int32(LINKS_DESKTOP_OK) {
                currentIssue = status == Int32(LINKS_DESKTOP_STALE_CURSOR)
                    ? .staleCursor : status == Int32(LINKS_DESKTOP_AUTHENTICATION)
                        ? .authenticationExpired : .dependencyOutage
            }
            lock.unlock()
            guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
            return fullSync ? .recoveryComplete : .pending
        }
    }

    func retryOutbox(transport: any IOSCoreTransport) throws {
        lock.lock()
        callbacks.transport = transport
        let status = links_desktop_core_retry_outbox(pointer)
        callbacks.transport = nil
        if status != Int32(LINKS_DESKTOP_OK) { currentIssue = .sendFailed }
        lock.unlock()
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
    }

    func resetReplayCursorForRecovery() throws {
        lock.lock()
        let status = links_desktop_core_reset_replay_cursor(pointer)
        lock.unlock()
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        currentIssue = nil
    }

    func maintainPreKeyInventory(accessToken: String, api: any IOSPreKeyAPI)
        async throws -> IOSPreKeyInventory {
        let existing = try await api.inventory(accessToken: accessToken)
        let curve = max(0, 100 - Int(existing.oneTimeCurvePreKeys))
        let kem = max(0, 100 - Int(existing.oneTimeKEMPreKeys))
        let uploadBytes = try generatePreKeyUpload(curve: UInt32(curve), kem: UInt32(kem))
        let upload = try IOSPreKeyUpload(protobuf: uploadBytes)
        let inventory = try await api.upload(accessToken: accessToken, upload: upload)
        let package = try generateMLSKeyPackage()
        if let api = api as? IOSPreKeyHTTPClient {
            try await api.uploadMLSKeyPackage(accessToken: accessToken, keyPackage: package)
        }
        return inventory
    }

    func initializeDirectConversation(
        conversationID: String,
        recipientUserID: String,
        recipientDevices: [IOSClaimedRecipientDevice],
        transport: any IOSCoreTransport) throws {
        lock.lock()
        callbacks.transport = transport
        defer {
            callbacks.transport = nil
            lock.unlock()
        }
        for recipient in recipientDevices {
            let status = recipient.userID.withCString { user in
                recipient.deviceID.withCString { device in
                    recipient.identityPublicKey.withUnsafeBytes { identityKey in
                        recipient.preKeyBundle.withUnsafeBytes { bundle in
                            recipient.mlsCredential.withUnsafeBytes { credential in
                                recipient.mlsKeyPackage.withUnsafeBytes { package in
                                    links_desktop_core_set_recipient(
                                        pointer,
                                        UnsafeRawPointer(user).assumingMemoryBound(to: UInt8.self), recipient.userID.utf8.count,
                                        UnsafeRawPointer(device).assumingMemoryBound(to: UInt8.self), recipient.deviceID.utf8.count,
                                        identityKey.bindMemory(to: UInt8.self).baseAddress!, recipient.identityPublicKey.count,
                                        bundle.bindMemory(to: UInt8.self).baseAddress!, recipient.preKeyBundle.count,
                                        credential.bindMemory(to: UInt8.self).baseAddress!, recipient.mlsCredential.count,
                                        package.bindMemory(to: UInt8.self).baseAddress!, recipient.mlsKeyPackage.count)
                                }
                            }
                        }
                    }
                }
            }
            guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        }
        let status = conversationID.withCString { conversation in
            recipientUserID.withCString { recipient in
                links_desktop_core_initialize_direct(
                    pointer,
                    UnsafeRawPointer(conversation).assumingMemoryBound(to: UInt8.self),
                    conversationID.utf8.count,
                    UnsafeRawPointer(recipient).assumingMemoryBound(to: UInt8.self),
                    recipientUserID.utf8.count)
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK) else {
            currentIssue = .sendFailed
            throw coreError(for: status)
        }
    }

    func resetDirectConversation(
        conversationID: String,
        recipientUserID: String,
        recipientDevices: [IOSClaimedRecipientDevice],
        transport: any IOSCoreTransport) throws {
        lock.lock()
        callbacks.transport = transport
        defer {
            callbacks.transport = nil
            lock.unlock()
        }
        for recipient in recipientDevices {
            let status = recipient.userID.withCString { user in
                recipient.deviceID.withCString { device in
                    recipient.identityPublicKey.withUnsafeBytes { identityKey in
                        recipient.preKeyBundle.withUnsafeBytes { bundle in
                            recipient.mlsCredential.withUnsafeBytes { credential in
                                recipient.mlsKeyPackage.withUnsafeBytes { package in
                                    links_desktop_core_set_recipient(
                                        pointer,
                                        UnsafeRawPointer(user).assumingMemoryBound(to: UInt8.self), recipient.userID.utf8.count,
                                        UnsafeRawPointer(device).assumingMemoryBound(to: UInt8.self), recipient.deviceID.utf8.count,
                                        identityKey.bindMemory(to: UInt8.self).baseAddress!, recipient.identityPublicKey.count,
                                        bundle.bindMemory(to: UInt8.self).baseAddress!, recipient.preKeyBundle.count,
                                        credential.bindMemory(to: UInt8.self).baseAddress!, recipient.mlsCredential.count,
                                        package.bindMemory(to: UInt8.self).baseAddress!, recipient.mlsKeyPackage.count)
                                }
                            }
                        }
                    }
                }
            }
            guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        }
        let status = conversationID.withCString { conversation in
            recipientUserID.withCString { recipient in
                links_desktop_core_reset_direct(
                    pointer,
                    UnsafeRawPointer(conversation).assumingMemoryBound(to: UInt8.self), conversationID.utf8.count,
                    UnsafeRawPointer(recipient).assumingMemoryBound(to: UInt8.self), recipientUserID.utf8.count)
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK) else {
            currentIssue = .sendFailed
            throw coreError(for: status)
        }
    }

    func sendText(conversationID: String, recipientUserID: String, text: String,
                  transport: any IOSCoreTransport) throws {
        lock.lock()
        callbacks.transport = transport
        let textLength = text.utf8.count
        let status = conversationID.withCString { conversation in
            recipientUserID.withCString { recipient in
                text.withCString { textBytes in
                    links_desktop_core_send_text(
                        pointer,
                        UnsafeRawPointer(conversation).assumingMemoryBound(to: UInt8.self), conversationID.utf8.count,
                        UnsafeRawPointer(recipient).assumingMemoryBound(to: UInt8.self), recipientUserID.utf8.count,
                        UnsafeRawPointer(textBytes).assumingMemoryBound(to: UInt8.self), textLength)
                }
            }
        }
        callbacks.transport = nil
        if status != Int32(LINKS_DESKTOP_OK) { currentIssue = .sendFailed }
        lock.unlock()
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
    }

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

    private func generatePreKeyUpload(curve: UInt32, kem: UInt32) throws -> Data {
        var buffer = Data(count: 1024 * 1024)
        let capacity = buffer.count
        var length = 0
        let status = buffer.withUnsafeMutableBytes { output in
            links_desktop_core_generate_prekey_upload(
                pointer, curve, kem,
                output.bindMemory(to: UInt8.self).baseAddress!, capacity, &length)
        }
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        buffer.removeSubrange(length..<buffer.count)
        return buffer
    }

    private func generateMLSKeyPackage() throws -> Data {
        var buffer = Data(count: 1024 * 1024)
        let capacity = buffer.count
        var length = 0
        let status = buffer.withUnsafeMutableBytes { output in
            links_desktop_core_generate_mls_key_package(
                pointer,
                output.bindMemory(to: UInt8.self).baseAddress!, capacity, &length)
        }
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        buffer.removeSubrange(length..<buffer.count)
        return buffer
    }
}

final class IOSRustCoreFactory: SharedClientCoreFactory {
    private let stateStore: IOSEncryptedStateStore
    private let secrets: IOSKeychainSecretProvider

    init(stateStore: IOSEncryptedStateStore,
         secrets: IOSKeychainSecretProvider) {
        self.stateStore = stateStore
        self.secrets = secrets
    }

    func makeCore(identity: SharedCoreIdentity,
                  signer: any SharedCoreIdentitySigner) throws -> any SharedClientCore {
        try IOSRustSharedCore(
            identity: identity,
            signer: signer,
            stateStore: stateStore,
            secrets: secrets)
    }
}

final class IOSDirectoryChatAdapter: IOSDirectChatDirectory {
    private let directoryClient: IOSUsernameAuthClient
    private let keyPackageProvider: IOSHTTPMLSKeyPackageProvider
    private let handles: () -> [String: String]

    init(directoryClient: IOSUsernameAuthClient,
         keyPackageProvider: IOSHTTPMLSKeyPackageProvider,
         handles: @escaping () -> [String: String]) {
        self.directoryClient = directoryClient
        self.keyPackageProvider = keyPackageProvider
        self.handles = handles
    }

    func queryRecipientDevices(accessToken: String, recipientUserID: String)
        async throws -> [IOSRecipientDeviceDescriptor] {
        guard let handle = handles()[recipientUserID] else {
            throw IOSPreKeyError.invalidRecipient
        }
        let directory = try await directoryClient.lookup(handle: handle)
        guard directory.userID == recipientUserID,
              !directory.devices.isEmpty,
              directory.devices.count <= 100 else {
            throw IOSPreKeyError.invalidRecipient
        }
        var descriptors = [IOSRecipientDeviceDescriptor]()
        descriptors.reserveCapacity(directory.devices.count)
        for device in directory.devices {
            let package = try await keyPackageProvider.keyPackage(
                accessToken: accessToken,
                userID: directory.userID,
                deviceID: device.deviceID,
                mlsNodeID: device.mlsNodeID,
                mlsCredential: device.mlsCredential)
            descriptors.append(try IOSRecipientDeviceDescriptor(
                userID: directory.userID,
                deviceID: device.deviceID,
                identityPublicKey: device.identityPublicKey,
                mlsCredential: device.mlsCredential,
                mlsKeyPackage: package))
        }
        return descriptors
    }
}
