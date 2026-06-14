import Foundation

public struct ModelPackOption: Identifiable, Equatable {
    public let id: String
    public let displayName: String
    public let runtime: String

    public init(id: String, displayName: String, runtime: String = "unknown") {
        self.id = id
        self.displayName = displayName
        self.runtime = runtime
    }
}

public enum ModelPackCatalog {
    public static func modelPacksRootURL() -> URL {
        if let appSupportURL = applicationSupportModelPacksRootURL() {
            try? FileManager.default.createDirectory(
                at: appSupportURL,
                withIntermediateDirectories: true
            )
            seedBundledModelPacks(into: appSupportURL)
            return appSupportURL
        }

        if let bundledURL = bundledModelPacksRootURL() {
            return bundledURL
        }

        return fallbackSourceModelPacksRootURL()
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
            let leftRank = runtimeSortRank(left.runtime)
            let rightRank = runtimeSortRank(right.runtime)
            if leftRank != rightRank {
                return leftRank < rightRank
            }

            return left.displayName.localizedCaseInsensitiveCompare(right.displayName)
                == .orderedAscending
        }
    }

    private static func applicationSupportModelPacksRootURL() -> URL? {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?
            .appendingPathComponent("SpeechClerk", isDirectory: true)
            .appendingPathComponent("ModelPacks", isDirectory: true)
    }

    private static func bundledModelPacksRootURL() -> URL? {
        if let appResourceURL = Bundle.main.resourceURL {
            let appResources = appResourceURL.appendingPathComponent("ModelPacks")
            if FileManager.default.fileExists(atPath: appResources.path) {
                return appResources
            }
        }

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

        let fallback = fallbackSourceModelPacksRootURL()
        return FileManager.default.fileExists(atPath: fallback.path) ? fallback : nil
    }

    private static func fallbackSourceModelPacksRootURL() -> URL {
        return URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .appendingPathComponent("Resources")
            .appendingPathComponent("ModelPacks")
    }

    private static func seedBundledModelPacks(into appSupportURL: URL) {
        guard let bundledURL = bundledModelPacksRootURL(),
            bundledURL != appSupportURL,
            let entries = try? FileManager.default.contentsOfDirectory(
                at: bundledURL,
                includingPropertiesForKeys: [.isDirectoryKey],
                options: [.skipsHiddenFiles]
            )
        else { return }

        for sourceURL in entries {
            let destinationURL = appSupportURL.appendingPathComponent(
                sourceURL.lastPathComponent,
                isDirectory: true
            )
            if !FileManager.default.fileExists(atPath: destinationURL.path) {
                try? FileManager.default.copyItem(at: sourceURL, to: destinationURL)
            }
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

        let runtime = object["runtime"] as? String ?? "unknown"
        return ModelPackOption(id: id, displayName: displayName, runtime: runtime)
    }

    private static func runtimeSortRank(_ runtime: String) -> Int {
        switch runtime {
        case "onnx":
            0
        case "fake":
            1
        default:
            2
        }
    }
}
