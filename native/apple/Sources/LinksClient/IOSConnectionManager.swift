import Foundation

public protocol IOSConnectionManagerDelegate: AnyObject {
    func connectionManager(_ manager: IOSConnectionManager, didChange state: IOSConnectionManager.State)
    func connectionManager(_ manager: IOSConnectionManager, didReceive frame: Data)
    func connectionManagerDidFail(_ manager: IOSConnectionManager)
    func connectionManagerDidDisconnect(_ manager: IOSConnectionManager)
}

/// TLS WebSocket lifecycle for the binary links.v1 transport.
public final class IOSConnectionManager {
    public enum State: Equatable, Sendable {
        case stopped
        case connecting
        case ready
        case failed
    }

    public static let heartbeatInterval: TimeInterval = 30
    public static let helloDeadline: TimeInterval = 5
    public static let initialBackoff: TimeInterval = 1
    public static let maximumBackoff: TimeInterval = 30
    public static let maximumFrameBytes = 1024 * 1024

    private final class SocketDelegate: NSObject, URLSessionWebSocketDelegate {
        weak var owner: IOSConnectionManager?

        func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask,
                        didOpenWithProtocol protocol: String?) {
            owner?.opened(webSocketTask, negotiatedProtocol: `protocol`)
        }

        func urlSession(_ session: URLSession, webSocketTask: URLSessionWebSocketTask,
                        didCloseWith closeCode: URLSessionWebSocketTask.CloseCode,
                        reason: Data?) {
            owner?.closed(webSocketTask)
        }
    }

    private let endpoint: URL
    private let helloProvider: () throws -> Data
    private weak var delegate: IOSConnectionManagerDelegate?
    private let callbackQueue: DispatchQueue
    private let stateQueue = DispatchQueue(label: "ai.links.ios.connection")
    private let socketDelegate = SocketDelegate()
    private lazy var urlSession = URLSession(
        configuration: .ephemeral,
        delegate: socketDelegate,
        delegateQueue: nil)

    private var socket: URLSessionWebSocketTask?
    private var reconnectWork: DispatchWorkItem?
    private var helloWork: DispatchWorkItem?
    private var stableResetWork: DispatchWorkItem?
    private var heartbeatTimer: DispatchSourceTimer?
    private var backoff = IOSConnectionManager.initialBackoff
    private var started = false
    private var isShutdown = false
    private var helloQueued = false
    private var currentState = State.stopped

    public init(endpoint: URL, helloProvider: @escaping () throws -> Data,
                delegate: IOSConnectionManagerDelegate,
                callbackQueue: DispatchQueue = .main) throws {
        guard endpoint.scheme?.lowercased() == "wss",
              endpoint.host?.isEmpty == false,
              endpoint.user == nil,
              endpoint.query == nil,
              endpoint.fragment == nil else {
            throw IOSConnectionError.invalidEndpoint
        }
        self.endpoint = endpoint
        self.helloProvider = helloProvider
        self.delegate = delegate
        self.callbackQueue = callbackQueue
        socketDelegate.owner = self
    }

    public var state: State {
        stateQueue.sync { currentState }
    }

    public var isConnected: Bool {
        stateQueue.sync { helloQueued && socket != nil }
    }

    public func start() {
        stateQueue.async {
            guard !self.isShutdown, !self.started else { return }
            self.started = true
            self.backoff = Self.initialBackoff
            self.setStateLocked(.connecting)
            self.connectLocked()
        }
    }

    public func stop() {
        stateQueue.async {
            guard self.started || self.socket != nil else { return }
            self.started = false
            self.reconnectWork?.cancel()
            self.reconnectWork = nil
            self.cancelSocketTimersLocked()
            let active = self.socket
            self.socket = nil
            self.helloQueued = false
            active?.cancel(with: .normalClosure, reason: nil)
            self.setStateLocked(.stopped)
        }
    }

    public func shutdown() {
        stateQueue.async {
            guard !self.isShutdown else { return }
            self.isShutdown = true
            self.started = false
            self.reconnectWork?.cancel()
            self.reconnectWork = nil
            self.cancelSocketTimersLocked()
            let active = self.socket
            self.socket = nil
            self.helloQueued = false
            active?.cancel(with: .normalClosure, reason: nil)
            self.setStateLocked(.stopped)
            self.urlSession.invalidateAndCancel()
        }
    }

    /// Send one complete binary frame. Bearer tokens belong in the caller's
    /// Hello frame, never in the WebSocket URL or a diagnostic callback.
    @discardableResult
    public func send(_ frame: Data) -> Bool {
        guard !frame.isEmpty, frame.count <= Self.maximumFrameBytes else { return false }
        var sent = false
        stateQueue.sync {
            guard self.helloQueued, let active = self.socket else { return }
            sent = true
            active.send(.data(frame)) { [weak self, weak active] error in
                guard let self, let active else { return }
                guard error != nil else { return }
                self.stateQueue.async {
                    self.failSocketLocked(active, reportFailure: true)
                }
            }
        }
        return sent
    }

    fileprivate func opened(_ task: URLSessionWebSocketTask, negotiatedProtocol: String?) {
        stateQueue.async {
            guard self.started, self.socket === task else {
                task.cancel(with: .normalClosure, reason: nil)
                return
            }
            guard negotiatedProtocol == "links.v1" else {
                self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                return
            }
            self.helloWork?.cancel()
            let deadline = DispatchWorkItem { [weak self, weak task] in
                guard let self, let task else { return }
                self.stateQueue.async {
                    guard self.socket === task, !self.helloQueued else { return }
                    self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                }
            }
            self.helloWork = deadline
            self.stateQueue.asyncAfter(deadline: .now() + Self.helloDeadline, execute: deadline)
            DispatchQueue.global(qos: .userInitiated).async { [weak self, weak task] in
                guard let self, let task else { return }
                let hello: Data
                do {
                    hello = try self.helloProvider()
                } catch {
                    self.stateQueue.async {
                        guard self.socket === task else { return }
                        self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                    }
                    return
                }
                self.stateQueue.async {
                    guard self.socket === task else { return }
                    guard !hello.isEmpty, hello.count <= Self.maximumFrameBytes else {
                        self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                        return
                    }
                    self.sendHelloLocked(task, hello: hello)
                }
            }
        }
    }

    fileprivate func closed(_ task: URLSessionWebSocketTask) {
        stateQueue.async {
            guard self.socket === task else { return }
            self.failSocketLocked(task, reportFailure: false)
        }
    }

    private func connectLocked() {
        guard started, !isShutdown, socket == nil else { return }
        let task = urlSession.webSocketTask(with: endpoint, protocols: ["links.v1"])
        socket = task
        helloQueued = false
        task.resume()
    }

    private func sendHelloLocked(_ task: URLSessionWebSocketTask, hello: Data) {
        task.send(.data(hello)) { [weak self, weak task] error in
            guard let self, let task else { return }
            self.stateQueue.async {
                guard self.socket === task else { return }
                if error != nil {
                    self.failSocketLocked(task, reportFailure: true)
                    return
                }
                self.helloQueued = true
                self.helloWork?.cancel()
                self.helloWork = nil
                self.installHeartbeatLocked(task)
                self.installStableResetLocked(task)
                self.setStateLocked(.ready)
                self.receiveLocked(task)
            }
        }
    }

    private func receiveLocked(_ task: URLSessionWebSocketTask) {
        task.receive { [weak self, weak task] result in
            guard let self, let task else { return }
            self.stateQueue.async {
                guard self.socket === task, self.helloQueued else { return }
                switch result {
                case .success(.data(let frame)):
                    guard !frame.isEmpty, frame.count <= Self.maximumFrameBytes else {
                        self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                        return
                    }
                    self.callbackQueue.async { [weak self] in
                        guard let self else { return }
                        self.delegate?.connectionManager(self, didReceive: frame)
                    }
                    self.receiveLocked(task)
                case .success(.string):
                    self.failSocketLocked(task, reportFailure: true, closeCode: .unsupportedData)
                case .failure:
                    self.failSocketLocked(task, reportFailure: true)
                @unknown default:
                    self.failSocketLocked(task, reportFailure: true, closeCode: .protocolError)
                }
            }
        }
    }

    private func installHeartbeatLocked(_ task: URLSessionWebSocketTask) {
        heartbeatTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: stateQueue)
        timer.schedule(deadline: .now() + Self.heartbeatInterval,
                       repeating: Self.heartbeatInterval)
        timer.setEventHandler { [weak self, weak task] in
            guard let self, let task, self.socket === task else { return }
            task.sendPing { [weak self, weak task] error in
                guard let self, let task else { return }
                self.stateQueue.async {
                    guard self.socket === task, error != nil else { return }
                    self.failSocketLocked(task, reportFailure: true)
                }
            }
        }
        timer.resume()
        heartbeatTimer = timer
    }

    private func installStableResetLocked(_ task: URLSessionWebSocketTask) {
        stableResetWork?.cancel()
        let work = DispatchWorkItem { [weak self, weak task] in
            guard let self, let task, self.socket === task else { return }
            self.backoff = Self.initialBackoff
        }
        stableResetWork = work
        stateQueue.asyncAfter(deadline: .now() + Self.heartbeatInterval, execute: work)
    }

    private func failSocketLocked(_ task: URLSessionWebSocketTask, reportFailure: Bool,
                                  closeCode: URLSessionWebSocketTask.CloseCode = .abnormalClosure) {
        guard socket === task else { return }
        socket = nil
        helloQueued = false
        cancelSocketTimersLocked()
        task.cancel(with: closeCode, reason: nil)
        if reportFailure {
            setStateLocked(.failed)
            notifyFailure()
        }
        notifyDisconnect()
        guard started, !isShutdown else {
            setStateLocked(.stopped)
            return
        }
        setStateLocked(.connecting)
        scheduleReconnectLocked()
    }

    private func scheduleReconnectLocked() {
        reconnectWork?.cancel()
        let upperBound = backoff
        let delay = upperBound == 0 ? 0 : Double.random(in: 0...upperBound)
        backoff = min(Self.maximumBackoff, max(Self.initialBackoff, upperBound * 2))
        let work = DispatchWorkItem { [weak self] in
            guard let self, self.started, !self.isShutdown, self.socket == nil else { return }
            self.connectLocked()
        }
        reconnectWork = work
        stateQueue.asyncAfter(deadline: .now() + delay, execute: work)
    }

    private func cancelSocketTimersLocked() {
        helloWork?.cancel()
        helloWork = nil
        stableResetWork?.cancel()
        stableResetWork = nil
        heartbeatTimer?.cancel()
        heartbeatTimer = nil
    }

    private func setStateLocked(_ next: State) {
        guard currentState != next else { return }
        currentState = next
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.connectionManager(self, didChange: next)
        }
    }

    private func notifyFailure() {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.connectionManagerDidFail(self)
        }
    }

    private func notifyDisconnect() {
        callbackQueue.async { [weak self] in
            guard let self else { return }
            self.delegate?.connectionManagerDidDisconnect(self)
        }
    }
}

public enum IOSConnectionError: Error {
    case invalidEndpoint
    case invalidHello
}
