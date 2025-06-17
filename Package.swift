// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "ContextRuntime",
    platforms: [.iOS(.v16)],
    products: [.library(name: "ContextRuntime", targets: ["ContextRuntime"])],
    targets: [
        .binaryTarget(
            name: "ContextRuntime",
            url: "https://github.com/jethro-djan/ctx-runtime/releases/download/v0.0.9/ContextRuntime.xcframework.zip",
            checksum: "00a31f304f92d65cfb7fba0ab13f289c3b5c1c2e4f7851f983421df5156661d5"
        )
    ]
)
