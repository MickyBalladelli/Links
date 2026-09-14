#if os(macOS)
import Foundation

/// Durable encrypted macOS records used by the shared Rust messaging core.
/// The enclosing `MacOSEncryptedStateStore` keeps these records out of
/// plaintext files and UserDefaults.
public enum MacOSDurableMessagingError: Error, Equatable {
    case invalidRecord
    case invalidIdentifier
    case cursorRegression
    case epochRegression
    case sequenceRegression
    case conflictingRecord
    case stateUnavailable
    case tooLarge
}

public struct MacOSMLSStateRecord: Codable, Equatable, Sendable {
    public let conversationID: String
    public let epoch: UInt64
    public let encryptedState: Data

    public init(conversationID: String, epoch: UInt64, encryptedState: Data) throws {
        guard MacOSDurableMessagingStore.isCanonicalID(conversationID),
              epoch <= MacOSDurableMessagingStore.maximumCursor,
              !encryptedState.isEmpty,
              encryptedState.count <= MacOSDurableMessagingStore.maximumMLSStateBytes else {
            throw MacOSDurableMessagingError.invalidRecord
        }
        self.conversationID = conversationID
        self.epoch = epoch
        self.encryptedState = encryptedState
    }
}

public struct MacOSInboxRecord: Codable, Equatable, Sendable {
    public let cursor: UInt64
    public let messageID: String?
    public let encryptedMessage: Data?
    public let tombstoneReason: Int?

    public init(cursor: UInt64, messageID: String?, encryptedMessage: Data?,
                tombstoneReason: Int? = nil) throws {
        guard cursor > 0 else { throw MacOSDurableMessagingError.invalidRecord }
        if let messageID, !MacOSDurableMessagingStore.isCanonicalID(messageID) {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        if let encryptedMessage {
            guard !encryptedMessage.isEmpty,
                  encryptedMessage.count <= MacOSDurableMessagingStore.maximumMessageBytes,
                  messageID != nil,
                  tombstoneReason == nil else {
                throw MacOSDurableMessagingError.invalidRecord
            }
        } else {
            guard messageID == nil, tombstoneReason == 1 || tombstoneReason == 2 else {
                throw MacOSDurableMessagingError.invalidRecord
            }
        }
        self.cursor = cursor
        self.messageID = messageID
        self.encryptedMessage = encryptedMessage
        self.tombstoneReason = tombstoneReason
    }

    public var isTombstone: Bool { encryptedMessage == nil }
}

public struct MacOSOutboxRecord: Codable, Equatable, Sendable {
    public let envelopeID: String
    public let encryptedFrame: Data
    public let createdAtMs: UInt64
    public let accepted: Bool

    public init(envelopeID: String, encryptedFrame: Data, createdAtMs: UInt64,
                accepted: Bool = false) throws {
        guard MacOSDurableMessagingStore.isCanonicalID(envelopeID),
              !encryptedFrame.isEmpty,
              encryptedFrame.count <= MacOSDurableMessagingStore.maximumFrameBytes else {
            throw MacOSDurableMessagingError.invalidRecord
        }
        self.envelopeID = envelopeID
        self.encryptedFrame = encryptedFrame
        self.createdAtMs = createdAtMs
        self.accepted = accepted
    }
}

/// One atomic mutation of the profile's encrypted messaging document.
public struct MacOSDurableMessagingTransaction {
    fileprivate var document: MacOSDurableMessagingStore.Document

    public var replayCursor: UInt64 { document.replayCursor }

    public func mlsState(conversationID: String) -> MacOSMLSStateRecord? {
        document.mlsStates[conversationID]
    }

    public func outboxRecords() -> [MacOSOutboxRecord] {
        document.outbox.sorted { $0.envelopeID < $1.envelopeID }
    }

    public func inboxRecords() -> [MacOSInboxRecord] {
        document.inbox.sorted { $0.cursor < $1.cursor }
    }

    public func conversationSequence(conversationID: String, senderDeviceID: String) throws -> UInt64 {
        let key = try MacOSDurableMessagingStore.sequenceKey(
            conversationID: conversationID, senderDeviceID: senderDeviceID)
        return document.sequences[key] ?? 0
    }

    /// Commit MLS state, decrypted-in-memory message ciphertext, and cursor in
    /// one encrypted file replacement. The caller must pass bytes encrypted
    /// by the shared core before invoking this method.
    public mutating func commitInbox(cursor: UInt64, records: [MacOSInboxRecord],
                                     mlsStates: [MacOSMLSStateRecord] = []) throws {
        guard cursor >= document.replayCursor else {
            throw MacOSDurableMessagingError.cursorRegression
        }
        for record in records where record.cursor > cursor {
            throw MacOSDurableMessagingError.invalidRecord
        }
        for record in records {
            if let index = document.inbox.firstIndex(where: { $0.cursor == record.cursor }),
               document.inbox[index] != record {
                throw MacOSDurableMessagingError.conflictingRecord
            }
            if !document.inbox.contains(where: { $0.cursor == record.cursor }) {
                document.inbox.append(record)
            }
        }
        for state in mlsStates {
            try saveMLSState(state)
        }
        document.replayCursor = cursor
    }

    public mutating func saveMLSState(_ state: MacOSMLSStateRecord) throws {
        if let current = document.mlsStates[state.conversationID], state.epoch < current.epoch {
            throw MacOSDurableMessagingError.epochRegression
        }
        document.mlsStates[state.conversationID] = state
    }

    /// Store the exact encrypted frame before sending it to the gateway.
    public mutating func enqueueOutbox(_ record: MacOSOutboxRecord) throws {
        if let index = document.outbox.firstIndex(where: { $0.envelopeID == record.envelopeID }) {
            guard document.outbox[index] == record else {
                throw MacOSDurableMessagingError.conflictingRecord
            }
            return
        }
        document.outbox.append(record)
    }

    public mutating func markOutboxAccepted(envelopeID: String) throws {
        guard let index = document.outbox.firstIndex(where: { $0.envelopeID == envelopeID }) else {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        let current = document.outbox[index]
        document.outbox[index] = try MacOSOutboxRecord(
            envelopeID: current.envelopeID,
            encryptedFrame: current.encryptedFrame,
            createdAtMs: current.createdAtMs,
            accepted: true)
    }

    public mutating func removeOutbox(envelopeID: String) throws {
        guard MacOSDurableMessagingStore.isCanonicalID(envelopeID) else {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        document.outbox.removeAll { $0.envelopeID == envelopeID }
    }

    @discardableResult
    public mutating func reserveMessageID(_ messageID: String) throws -> Bool {
        guard MacOSDurableMessagingStore.isCanonicalID(messageID) else {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        guard !document.messageIDs.contains(messageID) else { return false }
        document.messageIDs.append(messageID)
        return true
    }

    public mutating func reserveNextMessageID() throws -> String {
        for _ in 0..<4 {
            let messageID = UUID().uuidString.lowercased()
            if try reserveMessageID(messageID) { return messageID }
        }
        throw MacOSDurableMessagingError.conflictingRecord
    }

    public mutating func persistConversationSequence(conversationID: String,
                                                     senderDeviceID: String,
                                                     lastSequenceID: UInt64) throws {
        let key = try MacOSDurableMessagingStore.sequenceKey(
            conversationID: conversationID, senderDeviceID: senderDeviceID)
        guard lastSequenceID >= (document.sequences[key] ?? 0) else {
            throw MacOSDurableMessagingError.sequenceRegression
        }
        document.sequences[key] = lastSequenceID
    }

    public mutating func reserveNextConversationSequence(conversationID: String,
                                                        senderDeviceID: String) throws -> UInt64 {
        let key = try MacOSDurableMessagingStore.sequenceKey(
            conversationID: conversationID, senderDeviceID: senderDeviceID)
        let current = document.sequences[key] ?? 0
        guard current < MacOSDurableMessagingStore.maximumCursor else {
            throw MacOSDurableMessagingError.sequenceRegression
        }
        let next = current + 1
        document.sequences[key] = next
        return next
    }
}

/// Profile-scoped durable provider for MLS state and text transport state.
/// Every public mutation is an encrypted atomic document replacement. Use
/// `withTransaction` when MLS state, inbox, outbox, sequence, and cursor must
/// commit as one crash-safe unit.
public final class MacOSDurableMessagingStore: @unchecked Sendable {
    public static let maximumMLSStateBytes = 64 * 1024 * 1024
    public static let maximumMessageBytes = 512 * 1024
    public static let maximumFrameBytes = 1024 * 1024
    public static let maximumCursor = UInt64(Int64.max)
    public static let maximumRecordCount = 100_000

    public let profile: ClientProfile
    private let stateStore: MacOSEncryptedStateStore
    private let lock = NSLock()

    public init(profile: ClientProfile = .default,
                fileManager: FileManager = .default) throws {
        self.profile = profile
        stateStore = try MacOSEncryptedStateStore(
            profile: profile, namespace: "messaging", fileManager: fileManager)
    }

    public func withTransaction(
        _ body: (inout MacOSDurableMessagingTransaction) throws -> Void) throws {
        lock.lock()
        defer { lock.unlock() }
        var transaction = MacOSDurableMessagingTransaction(document: try readDocument())
        try body(&transaction)
        try transaction.document.validate(profile: profile)
        try writeDocument(transaction.document)
    }

    public func replayCursor() throws -> UInt64 {
        try read { $0.replayCursor }
    }

    public func mlsState(conversationID: String) throws -> MacOSMLSStateRecord? {
        guard Self.isCanonicalID(conversationID) else {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        return try read { $0.mlsStates[conversationID] }
    }

    public func inboxRecords() throws -> [MacOSInboxRecord] {
        try read { $0.inbox.sorted { $0.cursor < $1.cursor } }
    }

    public func outboxRecords() throws -> [MacOSOutboxRecord] {
        try read { $0.outbox.sorted { $0.envelopeID < $1.envelopeID } }
    }

    public func conversationSequence(conversationID: String,
                                     senderDeviceID: String) throws -> UInt64 {
        let key = try Self.sequenceKey(
            conversationID: conversationID, senderDeviceID: senderDeviceID)
        return try read { $0.sequences[key] ?? 0 }
    }

    @discardableResult
    public func reserveMessageID(_ messageID: String) throws -> Bool {
        var inserted = false
        try withTransaction { transaction in
            inserted = try transaction.reserveMessageID(messageID)
        }
        return inserted
    }

    public func reserveNextMessageID() throws -> String {
        var messageID = ""
        try withTransaction { transaction in
            messageID = try transaction.reserveNextMessageID()
        }
        return messageID
    }

    public func commitInbox(cursor: UInt64, records: [MacOSInboxRecord],
                            mlsStates: [MacOSMLSStateRecord] = []) throws {
        try withTransaction { transaction in
            try transaction.commitInbox(cursor: cursor, records: records, mlsStates: mlsStates)
        }
    }

    public func saveMLSState(_ state: MacOSMLSStateRecord) throws {
        try withTransaction { transaction in try transaction.saveMLSState(state) }
    }

    public func enqueueOutbox(_ record: MacOSOutboxRecord) throws {
        try withTransaction { transaction in try transaction.enqueueOutbox(record) }
    }

    public func markOutboxAccepted(envelopeID: String) throws {
        try withTransaction { transaction in try transaction.markOutboxAccepted(envelopeID: envelopeID) }
    }

    public func removeOutbox(envelopeID: String) throws {
        try withTransaction { transaction in try transaction.removeOutbox(envelopeID: envelopeID) }
    }

    public func persistConversationSequence(conversationID: String,
                                            senderDeviceID: String,
                                            lastSequenceID: UInt64) throws {
        try withTransaction { transaction in
            try transaction.persistConversationSequence(
                conversationID: conversationID,
                senderDeviceID: senderDeviceID,
                lastSequenceID: lastSequenceID)
        }
    }

    public func reserveNextConversationSequence(conversationID: String,
                                                senderDeviceID: String) throws -> UInt64 {
        var sequence: UInt64 = 0
        try withTransaction { transaction in
            sequence = try transaction.reserveNextConversationSequence(
                conversationID: conversationID, senderDeviceID: senderDeviceID)
        }
        return sequence
    }

    fileprivate static func isCanonicalID(_ value: String) -> Bool {
        guard let uuid = UUID(uuidString: value),
              uuid.uuidString.lowercased() == value else { return false }
        return uuid != UUID(uuidString: "00000000-0000-0000-0000-000000000000")
    }

    fileprivate static func sequenceKey(conversationID: String,
                                        senderDeviceID: String) throws -> String {
        guard isCanonicalID(conversationID), isCanonicalID(senderDeviceID) else {
            throw MacOSDurableMessagingError.invalidIdentifier
        }
        return "\(conversationID):\(senderDeviceID)"
    }

    private func read<T>(_ body: (Document) throws -> T) throws -> T {
        lock.lock()
        defer { lock.unlock() }
        return try body(readDocument())
    }

    private func readDocument() throws -> Document {
        let storedData: Data?
        do {
            storedData = try stateStore.read()
        } catch {
            throw MacOSDurableMessagingError.stateUnavailable
        }
        guard let data = storedData else { return Document(profileName: profile.name) }
        guard data.count <= MacOSEncryptedStateStore.maximumPlaintextBytes else {
            throw MacOSDurableMessagingError.tooLarge
        }
        do {
            let document = try JSONDecoder().decode(Document.self, from: data)
            try document.validate(profile: profile)
            return document
        } catch let error as MacOSDurableMessagingError {
            throw error
        } catch {
            throw MacOSDurableMessagingError.stateUnavailable
        }
    }

    private func writeDocument(_ document: Document) throws {
        do {
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            let data = try encoder.encode(document)
            guard data.count <= MacOSEncryptedStateStore.maximumPlaintextBytes else {
                throw MacOSDurableMessagingError.tooLarge
            }
            try stateStore.write(data)
        } catch let error as MacOSDurableMessagingError {
            throw error
        } catch {
            throw MacOSDurableMessagingError.stateUnavailable
        }
    }

    fileprivate struct Document: Codable {
        let version: UInt32
        let profileName: String
        var replayCursor: UInt64
        var mlsStates: [String: MacOSMLSStateRecord]
        var inbox: [MacOSInboxRecord]
        var outbox: [MacOSOutboxRecord]
        var messageIDs: [String]
        var sequences: [String: UInt64]

        init(profileName: String) {
            version = 1
            self.profileName = profileName
            replayCursor = 0
            mlsStates = [:]
            inbox = []
            outbox = []
            messageIDs = []
            sequences = [:]
        }

        func validate(profile: ClientProfile) throws {
            guard version == 1, profileName == profile.name,
                  replayCursor <= MacOSDurableMessagingStore.maximumCursor,
                  mlsStates.count <= MacOSDurableMessagingStore.maximumRecordCount,
                  inbox.count <= MacOSDurableMessagingStore.maximumRecordCount,
                  outbox.count <= MacOSDurableMessagingStore.maximumRecordCount,
                  messageIDs.count <= MacOSDurableMessagingStore.maximumRecordCount,
                  sequences.count <= MacOSDurableMessagingStore.maximumRecordCount else {
                throw MacOSDurableMessagingError.invalidRecord
            }
            for (conversationID, state) in mlsStates {
                guard conversationID == state.conversationID else {
                    throw MacOSDurableMessagingError.invalidRecord
                }
                _ = try MacOSMLSStateRecord(
                    conversationID: state.conversationID,
                    epoch: state.epoch,
                    encryptedState: state.encryptedState)
            }
            var cursors = Set<UInt64>()
            for record in inbox {
                guard record.cursor <= replayCursor,
                      cursors.insert(record.cursor).inserted else {
                    throw MacOSDurableMessagingError.conflictingRecord
                }
                _ = try MacOSInboxRecord(
                    cursor: record.cursor,
                    messageID: record.messageID,
                    encryptedMessage: record.encryptedMessage,
                    tombstoneReason: record.tombstoneReason)
            }
            var envelopeIDs = Set<String>()
            for record in outbox {
                guard envelopeIDs.insert(record.envelopeID).inserted else {
                    throw MacOSDurableMessagingError.conflictingRecord
                }
                _ = try MacOSOutboxRecord(
                    envelopeID: record.envelopeID,
                    encryptedFrame: record.encryptedFrame,
                    createdAtMs: record.createdAtMs,
                    accepted: record.accepted)
            }
            var messageIDs = Set<String>()
            for messageID in self.messageIDs {
                guard messageIDs.insert(messageID).inserted,
                      MacOSDurableMessagingStore.isCanonicalID(messageID) else {
                    throw MacOSDurableMessagingError.invalidIdentifier
                }
            }
            for (key, sequence) in sequences {
                let ids = key.split(separator: ":", omittingEmptySubsequences: true)
                guard ids.count == 2,
                      MacOSDurableMessagingStore.isCanonicalID(String(ids[0])),
                      MacOSDurableMessagingStore.isCanonicalID(String(ids[1])),
                      sequence <= MacOSDurableMessagingStore.maximumCursor else {
                    throw MacOSDurableMessagingError.invalidRecord
                }
            }
        }
    }
}
#endif
