import Foundation

final class RustDictationBridge: @unchecked Sendable {
    private let controller: DictationController

    init(modelPacksURL: URL) throws {
        controller = DictationController(
            config: DictationConfig(modelPacksDir: modelPacksURL.path)
        )
    }

    func loadModel(id: String) throws {
        try controller.loadModel(modelId: id)
    }

    func startRecording() throws {
        try controller.startRecording()
    }

    func pushAudio(samples: [Float], sampleRateHz: UInt32, channels: UInt16) throws {
        try controller.pushAudio(
            samples: samples,
            sampleRateHz: sampleRateHz,
            channels: channels
        )
    }

    func stopRecording() throws -> String? {
        try controller.stopRecording()?.text
    }

    func cancelRecording() throws {
        try controller.cancelRecording()
    }

    func setReplacement(pattern: String, replacement: String) throws {
        let rules = pattern.isEmpty
            ? []
            : [ReplacementRule(pattern: pattern, replacement: replacement)]
        try controller.setReplacementRules(rules: rules)
    }
}
