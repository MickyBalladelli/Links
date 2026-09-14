import CLinksIdentity
import Foundation

/// iOS facade for the shared native WASM mini-app runtime. This FFI facade uses
/// the runtime's deny-all permission host. It has no WASI, filesystem, clock,
/// randomness, identity, MLS, or private-key imports.
public final class IOSMiniAppSandbox {
    private var runtime: UnsafeMutableRawPointer?

    public init(module: Data) throws {
        var created: UnsafeMutableRawPointer?
        let status = module.withUnsafeBytes { bytes in
            links_sandbox_create(
                bytes.bindMemory(to: UInt8.self).baseAddress,
                bytes.count,
                &created)
        }
        guard status == LINKS_OK, let created else {
            throw IOSMiniAppSandboxError.invalidModule
        }
        runtime = created
    }

    deinit {
        close()
    }

    public func run(_ input: Data) throws -> Data {
        guard let runtime else { throw IOSMiniAppSandboxError.closed }
        guard input.count <= LINKS_SANDBOX_MAX_INPUT else {
            throw IOSMiniAppSandboxError.inputTooLarge
        }
        var output = Data(count: Int(LINKS_SANDBOX_MAX_OUTPUT))
        var outputLength = 0
        let status = output.withUnsafeMutableBytes { outputBytes in
            input.withUnsafeBytes { inputBytes in
                links_sandbox_run(
                    runtime,
                    inputBytes.bindMemory(to: UInt8.self).baseAddress,
                    inputBytes.count,
                    outputBytes.bindMemory(to: UInt8.self).baseAddress,
                    outputBytes.count,
                    &outputLength)
            }
        }
        guard status == LINKS_OK, outputLength >= 0, outputLength <= output.count else {
            throw IOSMiniAppSandboxError.executionFailed
        }
        output.removeSubrange(outputLength..<output.count)
        return output
    }

    public func close() {
        guard let runtime else { return }
        _ = links_sandbox_destroy(runtime)
        self.runtime = nil
    }
}

public enum IOSMiniAppSandboxError: Error {
    case invalidModule
    case inputTooLarge
    case executionFailed
    case closed
}
