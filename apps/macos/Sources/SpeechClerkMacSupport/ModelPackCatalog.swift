import Foundation

public struct ModelPackOption: Identifiable, Equatable {
    public let id: String
    public let displayName: String

    public init(id: String, displayName: String) {
        self.id = id
        self.displayName = displayName
    }
}

public enum ModelPackCatalog {
    public static func modelPacksRootURL() -> URL {
        if let resourceURL = Bundle.module.resourceURL {
            let copiedResources =
                resourceURL
                .appendingPathComponent("Resources")
                .appendingPathComponent("ModelPacks")
            if FileManager.default.fileExists(atPath: copiedResources.path) {
                return copiedResources
            }

            let directResources = resourceURL.appendingPathComponent("ModelPacks")
            if FileManager.default.fileExists(atPath: directResources.path) {
                return directResources
            }
        }

        return URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .appendingPathComponent("Resources")
            .appendingPathComponent("ModelPacks")
    }

    public static func loadModels(from rootURL: URL) -> [ModelPackOption] {
        guard
            let entries = try? FileManager.default.contentsOfDirectory(
                at: rootURL,
                includingPropertiesForKeys: [.isDirectoryKey],
                options: [.skipsHiddenFiles]
            )
        else {
            return []
        }

        return entries.compactMap(loadModel).sorted { left, right in
            left.displayName.localizedCaseInsensitiveCompare(right.displayName) == .orderedAscending
        }
    }

    private static func loadModel(from url: URL) -> ModelPackOption? {
        let manifestURL = url.appendingPathComponent("manifest.json")
        guard let data = try? Data(contentsOf: manifestURL),
            let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
            let id = object["modelId"] as? String,
            let displayName = object["displayName"] as? String
        else {
            return nil
        }

        return ModelPackOption(id: id, displayName: displayName)
    }
}
