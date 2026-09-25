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
    var onGroup: ((IOSGroupEvent) -> Void)?

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

private func iosRustGroup(
    _ context: UnsafeMutableRawPointer?,
    _ conversation: UnsafePointer<UInt8>?,
    _ conversationLength: Int,
    _ kind: UInt32,
    _ senderUser: UnsafePointer<UInt8>?,
    _ senderUserLength: Int,
    _ payload: UnsafePointer<UInt8>?,
    _ payloadLength: Int) -> Int32 {
    guard let box = callbackBox(context),
          let conversation = callbackData(conversation, conversationLength),
          let conversationID = String(data: conversation, encoding: .utf8),
          let senderUser = callbackData(senderUser, senderUserLength),
          let senderUserID = String(data: senderUser, encoding: .utf8),
          let payload = callbackData(payload, payloadLength),
          let text = String(data: payload, encoding: .utf8) else {
        return Int32(LINKS_DESKTOP_INVALID)
    }
    let eventKind: IOSGroupEvent.Kind
    switch Int(kind) {
    case Int(LINKS_DESKTOP_GROUP_JOINED): eventKind = .joined
    case Int(LINKS_DESKTOP_GROUP_RENAMED): eventKind = .renamed(name: text, byUserID: senderUserID)
    case Int(LINKS_DESKTOP_GROUP_MEMBERS_CHANGED): eventKind = .membersChanged
    case Int(LINKS_DESKTOP_GROUP_REMOVED): eventKind = .removed
    default: return Int32(LINKS_DESKTOP_INVALID)
    }
    box.onGroup?(IOSGroupEvent(conversationID: conversationID, kind: eventKind))
    return Int32(LINKS_DESKTOP_OK)
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
            abi_version: 2,
            context: Unmanaged.passUnretained(callbacks).toOpaque(),
            sign: iosRustSign,
            store_secret: iosRustStoreSecret,
            load_secret: iosRustLoadSecret,
            delete_secret: iosRustDeleteSecret,
            load_state: iosRustLoadState,
            save_state: iosRustSaveState,
            send_frame: iosRustSendFrame,
            on_text: iosRustText,
            identity_public_key: publicKeyTuple,
            on_group: iosRustGroup)
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
        let inventory: IOSPreKeyInventory
        do {
            inventory = try await api.upload(accessToken: accessToken, upload: upload)
        } catch IOSPreKeyError.conflict {
            inventory = existing
        }
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

    func hasRecipientDevices(for userID: String) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        var present: UInt8 = 0
        let status = userID.withCString { user in
            links_desktop_core_has_recipient(
                pointer,
                UnsafeRawPointer(user).assumingMemoryBound(to: UInt8.self),
                userID.utf8.count,
                &present)
        }
        return status == Int32(LINKS_DESKTOP_OK) && present != 0
    }

    // MARK: Groups

    func setGroupEventHandler(_ handler: @escaping (IOSGroupEvent) -> Void) {
        lock.lock()
        callbacks.onGroup = handler
        lock.unlock()
    }

    func registerRecipientDevices(_ devices: [IOSClaimedRecipientDevice]) throws {
        lock.lock()
        defer { lock.unlock() }
        for recipient in devices {
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
    }

    /// Run a group call with one or two UTF-8 arguments while a transport is
    /// attached for any frames the core sends.
    private func groupCall(_ conversationID: String, _ argument: String? = nil,
                           transport: (any IOSCoreTransport)? = nil,
                           _ body: (OpaquePointer, UnsafePointer<UInt8>, Int,
                                    UnsafePointer<UInt8>?, Int) -> Int32) throws {
        lock.lock()
        callbacks.transport = transport
        defer {
            callbacks.transport = nil
            lock.unlock()
        }
        let conversation = Array(conversationID.utf8)
        let extra = Array((argument ?? "").utf8)
        let status = conversation.withUnsafeBufferPointer { conversationBytes in
            extra.withUnsafeBufferPointer { extraBytes in
                body(pointer, conversationBytes.baseAddress!, conversation.count,
                     argument == nil ? nil : extraBytes.baseAddress, extra.count)
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK) else {
            if transport != nil { currentIssue = .sendFailed }
            throw coreError(for: status)
        }
    }

    private func groupList(_ conversationID: String,
                           _ body: (OpaquePointer, UnsafePointer<UInt8>, Int,
                                    UnsafeMutablePointer<UInt8>, Int, UnsafeMutablePointer<Int>) -> Int32)
        throws -> [String] {
        lock.lock()
        defer { lock.unlock() }
        let conversation = Array(conversationID.utf8)
        var buffer = [UInt8](repeating: 0, count: 64 * 1024)
        var length = 0
        let capacity = buffer.count
        let status = conversation.withUnsafeBufferPointer { conversationBytes in
            buffer.withUnsafeMutableBufferPointer { output in
                body(pointer, conversationBytes.baseAddress!, conversation.count,
                     output.baseAddress!, capacity, &length)
            }
        }
        guard status == Int32(LINKS_DESKTOP_OK) else { throw coreError(for: status) }
        guard let text = String(bytes: buffer.prefix(length), encoding: .utf8) else { return [] }
        return text.split(separator: "\n").map(String.init)
    }

    func createGroup(conversationID: String) throws {
        try groupCall(conversationID) { core, id, idLength, _, _ in
            links_desktop_core_create_group(core, id, idLength)
        }
    }

    func addGroupMembers(conversationID: String, userIDs: [String],
                         transport: any IOSCoreTransport) throws {
        try groupCall(conversationID, userIDs.joined(separator: "\n"), transport: transport) {
            core, id, idLength, users, usersLength in
            links_desktop_core_add_group_members(core, id, idLength, users, usersLength)
        }
    }

    func removeGroupMember(conversationID: String, userID: String,
                           transport: any IOSCoreTransport) throws {
        try groupCall(conversationID, userID, transport: transport) {
            core, id, idLength, user, userLength in
            links_desktop_core_remove_group_member(core, id, idLength, user, userLength)
        }
    }

    func disbandGroup(conversationID: String, transport: any IOSCoreTransport) throws {
        try groupCall(conversationID, transport: transport) { core, id, idLength, _, _ in
            links_desktop_core_disband_group(core, id, idLength)
        }
    }

    func leaveGroup(conversationID: String) throws {
        try groupCall(conversationID) { core, id, idLength, _, _ in
            links_desktop_core_leave_group(core, id, idLength)
        }
    }

    func sendGroupText(conversationID: String, text: String,
                       transport: any IOSCoreTransport) throws {
        try groupCall(conversationID, text, transport: transport) {
            core, id, idLength, text, textLength in
            links_desktop_core_send_group_text(core, id, idLength, text, textLength)
        }
    }

    func setGroupName(conversationID: String, name: String,
                      transport: any IOSCoreTransport) throws {
        try groupCall(conversationID, name, transport: transport) {
            core, id, idLength, name, nameLength in
            links_desktop_core_set_group_name(core, id, idLength, name, nameLength)
        }
    }

    func groupMembers(conversationID: String) throws -> [String] {
        try groupList(conversationID) { core, id, idLength, output, capacity, length in
            links_desktop_core_group_members(core, id, idLength, output, capacity, length)
        }
    }

    func groupUsersMissingRecipients(conversationID: String) throws -> [String] {
        try groupList(conversationID) { core, id, idLength, output, capacity, length in
            links_desktop_core_group_missing_recipients(core, id, idLength, output, capacity, length)
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
    private let onDirectoryResolved: (String, String, String?) -> Void

    init(directoryClient: IOSUsernameAuthClient,
         keyPackageProvider: IOSHTTPMLSKeyPackageProvider,
         onDirectoryResolved: @escaping (String, String, String?) -> Void = { _, _, _ in }) {
        self.directoryClient = directoryClient
        self.keyPackageProvider = keyPackageProvider
        self.onDirectoryResolved = onDirectoryResolved
    }

    func queryRecipientDevices(accessToken: String, recipientUserID: String)
        async throws -> [IOSRecipientDeviceDescriptor] {
        let directory = try await directoryClient.lookup(
            userID: recipientUserID, accessToken: accessToken)
        guard directory.userID == recipientUserID else {
            throw IOSPreKeyError.invalidRecipient
        }
        onDirectoryResolved(directory.userID, directory.handle, directory.displayName)
        guard !directory.devices.isEmpty,
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
