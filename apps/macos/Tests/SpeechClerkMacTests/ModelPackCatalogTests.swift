import Foundation
import SpeechClerkMacSupport

@main
struct ModelPackCatalogTestRunner {
    static func main() throws {
        let tests = ModelPackCatalogTests()
        try tests.loadsValidModelManifestsSortedByDisplayName()
        try tests.ignoresEntriesWithoutValidManifestIdentity()
    }
}

struct ModelPackCatalogTests {
    func loadsValidModelManifestsSortedByDisplayName() throws {
        let rootURL = try makeTemporaryDirectory()
        defer {
            try? FileManager.default.removeItem(at: rootURL)
        }

        try writeManifest(
            rootURL: rootURL,
            directoryName: "zeta",
            modelID: "zeta-model",
            displayName: "Zeta Model"
        )
        try writeManifest(
            rootURL: rootURL,
            directoryName: "alpha",
            modelID: "alpha-model",
            displayName: "Alpha Model"
        )

        let models = ModelPackCatalog.loadModels(from: rootURL)

        try expectEqual(
            models,
            [
                ModelPackOption(id: "alpha-model", displayName: "Alpha Model"),
                ModelPackOption(id: "zeta-model", displayName: "Zeta Model"),
            ]
        )
    }

    func ignoresEntriesWithoutValidManifestIdentity() throws {
        let rootURL = try makeTemporaryDirectory()
        defer {
            try? FileManager.default.removeItem(at: rootURL)
        }

        let invalidURL = rootURL.appendingPathComponent("invalid", isDirectory: true)
        try FileManager.default.createDirectory(at: invalidURL, withIntermediateDirectories: true)
        try Data(#"{"displayName":"Missing ID"}"#.utf8)
            .write(to: invalidURL.appendingPathComponent("manifest.json"))

        try writeManifest(
            rootURL: rootURL,
            directoryName: "valid",
            modelID: "valid-model",
            displayName: "Valid Model"
        )

        let models = ModelPackCatalog.loadModels(from: rootURL)

        try expectEqual(models, [ModelPackOption(id: "valid-model", displayName: "Valid Model")])
    }

    private func makeTemporaryDirectory() throws -> URL {
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("SpeechClerkMacTests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        return url
    }

    private func writeManifest(
        rootURL: URL,
        directoryName: String,
        modelID: String,
        displayName: String
    ) throws {
        let modelURL = rootURL.appendingPathComponent(directoryName, isDirectory: true)
        try FileManager.default.createDirectory(at: modelURL, withIntermediateDirectories: true)

        let manifest = """
            {
              "modelId": "\(modelID)",
              "displayName": "\(displayName)"
            }
            """
        try Data(manifest.utf8).write(to: modelURL.appendingPathComponent("manifest.json"))
    }

    private func expectEqual<T: Equatable>(_ actual: T, _ expected: T) throws {
        guard actual == expected else {
            throw TestFailure("expected \(expected), got \(actual)")
        }
    }
}

struct TestFailure: Error, CustomStringConvertible {
    let description: String

    init(_ description: String) {
        self.description = description
    }
}
