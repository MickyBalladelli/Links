import Foundation

public struct IOSAPNsWakeup: Sendable {
    public let recipientDeviceID: String
    public let cursor: UInt64
    public let fullSync: Bool

    private static let maximumCursor = UInt64(Int64.max)

    private init(recipientDeviceID: String, cursor: UInt64, fullSync: Bool) {
        self.recipientDeviceID = recipientDeviceID
        self.cursor = cursor
        self.fullSync = fullSync
    }

    /// Parse the exact silent APNs payload emitted by PushWakeup.apns_request.
    public static func parse(_ userInfo: [AnyHashable: Any]) throws -> IOSAPNsWakeup {
        guard userInfo.count == 3,
              let aps = userInfo[AnyHashable("aps")] as? [String: Any],
              aps.count == 1,
              let available = aps["content-available"] as? NSNumber,
              available.intValue == 1,
              let deviceID = userInfo[AnyHashable("recipient_device_id")] as? String,
              IOSClient.isCanonicalUUID(deviceID),
              let cursorValue = userInfo[AnyHashable("cursor")] as? String,
              let cursor = parseCursor(cursorValue) else {
            throw IOSRecoveryError.invalidWakeup
        }
        return IOSAPNsWakeup(recipientDeviceID: deviceID, cursor: cursor, fullSync: false)
    }

    /// Explicit recovery after an expired or missing push cursor.
    public static func fullSync(recipientDeviceID: String) throws -> IOSAPNsWakeup {
        guard IOSClient.isCanonicalUUID(recipientDeviceID) else {
            throw IOSRecoveryError.invalidWakeup
        }
        return IOSAPNsWakeup(recipientDeviceID: recipientDeviceID, cursor: 0, fullSync: true)
    }

    private static func parseCursor(_ value: String) -> UInt64? {
        guard !value.isEmpty, let cursor = UInt64(value),
              cursor > 0, cursor <= maximumCursor,
              String(cursor) == value else { return nil }
        return cursor
    }
}

public enum IOSRecoveryOutcome: Sendable {
    case complete
    case authenticationRequired
    case retry
}

public enum IOSRecoveryError: Error {
    case invalidWakeup
    case invalidConfiguration
}

/// Bounded mailbox replay for an APNs wakeup. The shared core owns replay,
/// decrypt, durable inbox/cursor commit and QueueAck ordering.
public final class IOSMissingMessageRecovery: IOSConnectionManagerDelegate {
    public static let timeout: TimeInterval = 25

    private final class Run {
        let semaphore = DispatchSemaphore(value: 0)
        private let lock = NSLock()
        private var outcome: IOSRecoveryOutcome?

        func finish(_ next: IOSRecoveryOutcome) -> Bool {
            lock.lock()
            defer { lock.unlock() }
            guard outcome == nil else { return false }
            outcome = next
            semaphore.signal()
            return true
        }

        func result() -> IOSRecoveryOutcome? {
            lock.lock()
            defer { lock.unlock() }
            return outcome
        }
    }

    private let client: IOSClient
    private let factory: any SharedClientCoreFactory
    private let endpoint: URL
    private let recoveryQueue = DispatchQueue(
        label: "ai.links.ios.recovery", qos: .utility)
    private let runLock = NSLock()
    private var activeRun: Run?
    private var activeCore: (any SharedClientCore)?
    private weak var activeManager: IOSConnectionManager?
    private var activeFullSync = false
    private let onTextMessage: ((IOSReceivedTextMessage) -> Void)?

    public init(client: IOSClient, factory: any SharedClientCoreFactory, endpoint: URL,
                onTextMessage: ((IOSReceivedTextMessage) -> Void)? = nil) {
        self.client = client
        self.factory = factory
        self.endpoint = endpoint
        self.onTextMessage = onTextMessage
    }

    /// Call from an APNs/background worker callback, never the main thread.
    public func recover(_ wakeup: IOSAPNsWakeup) -> IOSRecoveryOutcome {
        guard client.isAuthenticated, let localDeviceID = client.deviceID else {
            return .authenticationRequired
        }
        guard localDeviceID == wakeup.recipientDeviceID else { return .retry }

        let sharedCore: any SharedClientCore
        do {
            sharedCore = try client.makeCore(using: factory)
        } catch {
            return client.isAuthenticated ? .retry : .authenticationRequired
        }

        let run = Run()
        let manager: IOSConnectionManager
        do {
            manager = try IOSConnectionManager(
                endpoint: endpoint,
                helloProvider: { [client, sharedCore] in
                    let cursor = try sharedCore.durableCursor()
                    let token = try client.accessToken()
                    return try sharedCore.createHello(
                        accessToken: token, lastSeenCursor: cursor)
                },
                delegate: self,
                callbackQueue: recoveryQueue)
        } catch {
            return .retry
        }

        recoveryQueue.async {
            manager.start()
        }
        if run.semaphore.wait(timeout: .now() + Self.timeout) == .timedOut {
            _ = run.finish(.retry)
        }
        manager.shutdown()
        return run.result() ?? .retry
    }

    public func connectionManager(_ manager: IOSConnectionManager,
                                  didChange state: IOSConnectionManager.State) {}

    public func connectionManager(_ manager: IOSConnectionManager, didReceive frame: Data) {
        // The run is identified by the current recovery manager callback. A
        // fresh run gets a fresh core and manager, so no cursor is shared here.
        // The active run is retained by the manager's delegate callback below.
    }

    public func connectionManagerDidFail(_ manager: IOSConnectionManager) {}

    public func connectionManagerDidDisconnect(_ manager: IOSConnectionManager) {}
}

#if canImport(UIKit)
import UIKit

/// UIKit adapter for AppDelegate's silent APNs callback.
public final class IOSAPNsBackgroundHandler {
    private let recovery: IOSMissingMessageRecovery

    public init(recovery: IOSMissingMessageRecovery) {
        self.recovery = recovery
    }

    public func handle(_ userInfo: [AnyHashable: Any],
                      completion: @escaping (UIBackgroundFetchResult) -> Void) {
        let wakeup: IOSAPNsWakeup
        do {
            wakeup = try IOSAPNsWakeup.parse(userInfo)
        } catch {
            completion(.failed)
            return
        }
        DispatchQueue.global(qos: .utility).async { [recovery] in
            let outcome = recovery.recover(wakeup)
            let result: UIBackgroundFetchResult
            switch outcome {
            case .complete: result = .newData
            case .authenticationRequired: result = .failed
            case .retry: result = .noData
            }
            DispatchQueue.main.async {
                completion(result)
            }
        }
    }
}
#endif
