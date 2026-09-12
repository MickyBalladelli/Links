// swift-tools-version: 5.9
import PackageDescription
let package = Package(
    name: "LinksKeyStore",
    platforms: [.iOS(.v16), .macOS(.v13)],
    products: [.library(name: "LinksKeyStore", targets: ["LinksKeyStore"])],
    targets: [
        .target(name: "LinksKeyStore"),
        .testTarget(name: "LinksKeyStoreTests", dependencies: ["LinksKeyStore"])
    ]
)
