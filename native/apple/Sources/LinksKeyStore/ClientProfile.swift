import Foundation

public enum ClientProfileError: Error, Equatable {
    case invalidName
}

/// Stable namespace for one local Links client identity.
public struct ClientProfile: Equatable, Hashable, Sendable {
    public let name: String

    public static let `default` = ClientProfile(uncheckedName: "default")

    public init(name: String) throws {
        let normalized = name.lowercased()
        let scalars = normalized.unicodeScalars
        guard !normalized.isEmpty, normalized.utf8.count <= 64,
              scalars.first.map({ $0.value >= 97 && $0.value <= 122 }) == true,
              scalars.allSatisfy({
                  ($0.value >= 97 && $0.value <= 122)
                      || ($0.value >= 48 && $0.value <= 57)
                      || $0.value == 46 || $0.value == 95 || $0.value == 45
              }) else {
            throw ClientProfileError.invalidName
        }
        self.name = normalized
    }

    private init(uncheckedName: String) {
        name = uncheckedName
    }
}
