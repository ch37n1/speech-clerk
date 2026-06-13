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
    @Published var microphoneStateText = "Not requested"
    @Published var pasteControlStateText = "Not checked"
    @Published var replacementPattern: String
    @Published var replacementValue: String
    @Published var lastTranscript = ""

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
        replacementPattern = UserDefaults.standard.string(forKey: "replacementPattern") ?? "parakeet"
        replacementValue = UserDefaults.standard.string(forKey: "replacementValue") ?? "Canary"
        bridge = try? RustDictationBridge(modelPacksURL: modelPacksRootURL)
        refreshMicrophoneState()
        refreshPasteControlState()
    }

    func loadSelectedModel() {
        guard !selectedModelID.isEmpty else {
            statusText = "No model"
            return
        }

        do {
            try bridge?.loadModel(id: selectedModelID)
            modelLoaded = true
            statusText = "Model loaded"
            applyReplacementRule()
        } catch {
            modelLoaded = false
            statusText = "Model error"
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
}
