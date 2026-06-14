import AVFoundation
import AppKit
import Foundation
import SpeechClerkMacSupport

@MainActor
final class DictationViewModel: ObservableObject {
    @Published var models: [ModelPackOption]
    @Published var selectedModelID: String
    @Published var isRecording = false
    @Published var modelLoaded = false
    @Published var statusText = "Idle"
    @Published var modelStateText = ""
    @Published var microphoneStateText = "Not requested"
    @Published var pasteControlStateText = "Not checked"
    @Published var replacementPattern: String
    @Published var replacementValue: String
    @Published var lastTranscript = ""
    @Published var benchmarkStatusText = "Not run"
    @Published var benchmarkResultsText = ""
    @Published var benchmarkIsRunning = false

    private let modelPacksRootURL: URL
    private let audioCapture = AudioCapture()
    private let applicationTracker = ActiveApplicationTracker()
    private var bridge: RustDictationBridge?

    var canRecord: Bool {
        modelLoaded && microphoneIsAuthorized
    }

    private var microphoneIsAuthorized: Bool {
        AVCaptureDevice.authorizationStatus(for: .audio) == .authorized
    }

    init() {
        let rootURL = ModelPackCatalog.modelPacksRootURL()
        let loadedModels = ModelPackCatalog.loadModels(from: rootURL)
        modelPacksRootURL = rootURL
        models = loadedModels
        selectedModelID = loadedModels.first?.id ?? ""
        modelStateText =
            loadedModels.isEmpty
            ? "Install a local model pack"
            : "Ready to load \(loadedModels.first?.displayName ?? "selected model")"
        replacementPattern = UserDefaults.standard.string(forKey: "replacementPattern") ?? "parakeet"
        replacementValue = UserDefaults.standard.string(forKey: "replacementValue") ?? "Canary"
        bridge = try? RustDictationBridge(modelPacksURL: modelPacksRootURL)
        refreshMicrophoneState()
        refreshPasteControlState()
    }

    func loadSelectedModel() {
        guard !selectedModelID.isEmpty else {
            statusText = "No model"
            modelStateText = "Install a local model pack"
            return
        }

        do {
            try bridge?.loadModel(id: selectedModelID)
            modelLoaded = true
            statusText = "Model loaded"
            modelStateText = "Loaded \(displayName(for: selectedModelID))"
            applyReplacementRule()
        } catch {
            modelLoaded = false
            statusText = "Model error"
            modelStateText = "Check manifest files and SHA-256 checksums"
        }
    }

    func requestMicrophoneAccess() {
        AVCaptureDevice.requestAccess(for: .audio) { [weak self] _ in
            Task { @MainActor in
                self?.refreshMicrophoneState()
            }
        }
    }

    func requestPasteControlAccess() {
        if ClipboardInserter.requestKeyboardPastePermission() {
            pasteControlStateText = "Allowed"
        } else {
            refreshPasteControlState()
        }
    }

    func applyReplacementRule() {
        UserDefaults.standard.set(replacementPattern, forKey: "replacementPattern")
        UserDefaults.standard.set(replacementValue, forKey: "replacementValue")

        do {
            try bridge?.setReplacement(pattern: replacementPattern, replacement: replacementValue)
            statusText = modelLoaded ? "Ready" : statusText
        } catch {
            statusText = "Settings error"
        }
    }

    func runBenchmark() {
        guard !benchmarkIsRunning else { return }
        guard !selectedModelID.isEmpty else {
            benchmarkStatusText = "No model"
            benchmarkResultsText = ""
            return
        }

        let modelID = selectedModelID
        let rootURL = modelPacksRootURL
        let currentReplacementPattern = replacementPattern
        let currentReplacementValue = replacementValue
        benchmarkIsRunning = true
        benchmarkStatusText = "Running"
        benchmarkResultsText = ""

        Task.detached {
            do {
                let summary = try Self.performBenchmark(
                    modelPacksRootURL: rootURL,
                    modelID: modelID,
                    replacementPattern: currentReplacementPattern,
                    replacementValue: currentReplacementValue
                )
                await MainActor.run {
                    self.benchmarkIsRunning = false
                    self.benchmarkStatusText = "Complete"
                    self.benchmarkResultsText = summary.displayText
                }
            } catch {
                await MainActor.run {
                    self.benchmarkIsRunning = false
                    self.benchmarkStatusText = "Benchmark error"
                    self.benchmarkResultsText = ""
                }
            }
        }
    }

    func toggleRecording() {
        if isRecording {
            stopRecording()
        } else {
            startRecording()
        }
    }

    func cancelRecording() {
        audioCapture.stop()

        do {
            try bridge?.cancelRecording()
        } catch {
            statusText = "Cancel error"
        }

        isRecording = false
        statusText = "Canceled"
    }

    private func startRecording() {
        refreshMicrophoneState()
        guard canRecord else {
            statusText = microphoneIsAuthorized ? "Load model" : "Mic blocked"
            return
        }

        guard let bridge else {
            statusText = "Bridge missing"
            return
        }

        do {
            try bridge.startRecording()
            try audioCapture.start { samples, sampleRateHz, channels in
                do {
                    try bridge.pushAudio(
                        samples: samples,
                        sampleRateHz: sampleRateHz,
                        channels: channels
                    )
                } catch {
                    Task { @MainActor in
                        self.statusText = "Audio error"
                    }
                }
            }
            isRecording = true
            statusText = "Recording"
            lastTranscript = ""
        } catch {
            audioCapture.stop()
            isRecording = false
            statusText = "Start error"
        }
    }

    private func stopRecording() {
        audioCapture.stop()
        statusText = "Transcribing"

        do {
            let transcript = try bridge?.stopRecording()
            isRecording = false

            if let transcript, !transcript.isEmpty {
                lastTranscript = transcript
                if ClipboardInserter.paste(
                    transcript,
                    into: applicationTracker.lastExternalApplication
                ) {
                    statusText = "Inserted"
                    refreshPasteControlState()
                } else {
                    statusText = "Paste blocked"
                    refreshPasteControlState()
                }
            } else {
                statusText = "No audio"
            }
        } catch {
            isRecording = false
            statusText = "Stop error"
        }
    }

    private func displayName(for modelID: String) -> String {
        models.first { $0.id == modelID }?.displayName ?? modelID
    }

    private func refreshMicrophoneState() {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            microphoneStateText = "Allowed"
        case .denied, .restricted:
            microphoneStateText = "Blocked"
        case .notDetermined:
            microphoneStateText = "Not requested"
        @unknown default:
            microphoneStateText = "Unknown"
        }
    }

    private func refreshPasteControlState() {
        pasteControlStateText = ClipboardInserter.hasKeyboardPastePermission ? "Allowed" : "Blocked"
    }

    private nonisolated static func performBenchmark(
        modelPacksRootURL: URL,
        modelID: String,
        replacementPattern: String,
        replacementValue: String
    ) throws -> BenchmarkSummary {
        let bridge = try RustDictationBridge(modelPacksURL: modelPacksRootURL)
        let samples = makeBenchmarkSamples()
        var peakMemoryBytes = residentMemoryBytes() ?? 0

        let loadStart = DispatchTime.now().uptimeNanoseconds
        try bridge.loadModel(id: modelID)
        let modelLoadMs = elapsedMilliseconds(since: loadStart)
        peakMemoryBytes = max(peakMemoryBytes, residentMemoryBytes() ?? 0)

        try bridge.setReplacement(pattern: replacementPattern, replacement: replacementValue)

        var finalizationLatencies: [Double] = []
        var endToEndLatencies: [Double] = []
        var transcript = ""

        for _ in 0..<3 {
            let runStart = DispatchTime.now().uptimeNanoseconds
            try bridge.startRecording()
            try bridge.pushAudio(samples: samples, sampleRateHz: 16_000, channels: 1)
            let finalizationStart = DispatchTime.now().uptimeNanoseconds
            transcript = try bridge.stopRecording() ?? ""
            finalizationLatencies.append(elapsedMilliseconds(since: finalizationStart))
            endToEndLatencies.append(elapsedMilliseconds(since: runStart))
            peakMemoryBytes = max(peakMemoryBytes, residentMemoryBytes() ?? 0)
        }

        return BenchmarkSummary(
            modelLoadMs: modelLoadMs,
            endToEndMeanMs: mean(endToEndLatencies),
            finalizationP50Ms: percentile(finalizationLatencies, percentile: 0.50),
            finalizationP95Ms: percentile(finalizationLatencies, percentile: 0.95),
            rtf: mean(finalizationLatencies) / 5_000.0,
            peakMemoryBytes: peakMemoryBytes,
            transcript: transcript
        )
    }

    private nonisolated static func makeBenchmarkSamples() -> [Float] {
        let sampleCount = 16_000 * 5
        var samples: [Float] = []
        samples.reserveCapacity(sampleCount)

        for index in 0..<sampleCount {
            let highPhase = (index / 80).isMultiple(of: 2)
            samples.append(highPhase ? 0.08 : -0.08)
        }

        return samples
    }

    private nonisolated static func elapsedMilliseconds(since start: UInt64) -> Double {
        let elapsedNanoseconds = DispatchTime.now().uptimeNanoseconds.saturatingSubtracting(start)
        return Double(elapsedNanoseconds) / 1_000_000.0
    }

    private nonisolated static func mean(_ values: [Double]) -> Double {
        guard !values.isEmpty else { return 0 }
        return values.reduce(0, +) / Double(values.count)
    }

    private nonisolated static func percentile(_ values: [Double], percentile: Double) -> Double {
        guard !values.isEmpty else { return 0 }
        let sorted = values.sorted()
        let index = max(0, min(sorted.count - 1, Int((Double(sorted.count) * percentile).rounded(.up)) - 1))
        return sorted[index]
    }

    private nonisolated static func residentMemoryBytes() -> UInt64? {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/bin/ps")
        process.arguments = [
            "-o",
            "rss=",
            "-p",
            "\(ProcessInfo.processInfo.processIdentifier)",
        ]

        let pipe = Pipe()
        process.standardOutput = pipe
        try? process.run()
        process.waitUntilExit()

        guard process.terminationStatus == 0 else { return nil }
        let data = pipe.fileHandleForReading.readDataToEndOfFile()
        guard let text = String(data: data, encoding: .utf8),
            let kilobytes = UInt64(text.trimmingCharacters(in: .whitespacesAndNewlines))
        else { return nil }

        return kilobytes * 1_024
    }
}

private struct BenchmarkSummary {
    let modelLoadMs: Double
    let endToEndMeanMs: Double
    let finalizationP50Ms: Double
    let finalizationP95Ms: Double
    let rtf: Double
    let peakMemoryBytes: UInt64
    let transcript: String

    var displayText: String {
        """
        Model load: \(Self.format(modelLoadMs)) ms
        End-to-end mean: \(Self.format(endToEndMeanMs)) ms
        Finalization P50: \(Self.format(finalizationP50Ms)) ms
        Finalization P95: \(Self.format(finalizationP95Ms)) ms
        RTF: \(Self.format(rtf))
        Peak memory: \(peakMemoryBytes / 1_048_576) MB
        Transcript: \(transcript)
        """
    }

    private static func format(_ value: Double) -> String {
        String(format: "%.2f", value)
    }
}

extension UInt64 {
    fileprivate func saturatingSubtracting(_ other: UInt64) -> UInt64 {
        self >= other ? self - other : 0
    }
}
