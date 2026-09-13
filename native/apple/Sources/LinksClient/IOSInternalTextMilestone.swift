import Foundation

/// Feature contract for the first internal iOS build. This target exposes
/// one-to-one text only; groups, media and calls are deliberately unavailable.
public enum IOSInternalTextMilestone {
    public enum Feature: Sendable {
        case oneToOneText
        case groups
        case media
        case calls
    }

    public static let identifier = "ios-text-internal-1"
    public static let version = "0.1.0-internal"
    public static let minimumOS = "iOS 16"
    public static let maximumTextBytes = 64 * 1024

    public static func isEnabled(_ feature: Feature) -> Bool {
        switch feature {
        case .oneToOneText: return true
        case .groups, .media, .calls: return false
        }
    }

    public static func validateText(_ text: String) throws {
        guard isEnabled(.oneToOneText), !text.isEmpty,
              text.utf8.count <= maximumTextBytes else {
            throw IOSInternalMilestoneError.textUnavailable
        }
    }
}

public enum IOSInternalMilestoneError: Error {
    case textUnavailable
}
