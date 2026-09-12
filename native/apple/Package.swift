// swift-tools-version: 5.9
import PackageDescription
import Foundation
// Build Rust first. Cross builds must select the matching target/profile directory.
let rustLibraryDirectory = ProcessInfo.processInfo.environment["LINKS_IDENTITY_LIB_DIR"]
    ?? URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .appendingPathComponent("../../target/debug").standardizedFileURL.path
let package = Package(
    name: "LinksKeyStore",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [.library(name: "LinksKeyStore", targets: ["LinksKeyStore"])],
    targets: [
        .systemLibrary(name: "CLinksIdentity"),
        .target(name: "LinksKeyStore", dependencies: ["CLinksIdentity"], linkerSettings: [
            .unsafeFlags(["-L", rustLibraryDirectory]),
            .linkedLibrary("links_identity_ffi"),
            .linkedFramework("Security")
        ]),
        .testTarget(name: "LinksKeyStoreTests", dependencies: ["LinksKeyStore"])
    ]
)
