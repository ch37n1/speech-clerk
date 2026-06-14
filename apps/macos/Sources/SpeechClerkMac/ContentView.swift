import SpeechClerkMacSupport
import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var viewModel: DictationViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Label("Speech Clerk", systemImage: "waveform")
                    .font(.title2)
                    .fontWeight(.semibold)

                Spacer()

                Label(viewModel.statusText, systemImage: viewModel.isRecording ? "record.circle" : "checkmark.circle")
                    .foregroundStyle(viewModel.isRecording ? .red : .secondary)
                    .accessibilityIdentifier("app-status")
            }

            Divider()

            VStack(alignment: .leading, spacing: 8) {
                Text("Model")
                    .font(.headline)

                Picker("Model", selection: $viewModel.selectedModelID) {
                    ForEach(viewModel.models) { model in
                        Text(model.displayName).tag(model.id)
                    }
                }
                .accessibilityIdentifier("model-picker")
                .labelsHidden()
                .frame(maxWidth: .infinity, alignment: .leading)

                Button {
                    viewModel.loadSelectedModel()
                } label: {
                    Label("Load Model", systemImage: "square.and.arrow.down")
                }
                .accessibilityIdentifier("load-model-button")
                .disabled(viewModel.selectedModelID.isEmpty)
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("Permissions")
                    .font(.headline)

                HStack {
                    Button {
                        viewModel.requestMicrophoneAccess()
                    } label: {
                        Label("Allow Microphone", systemImage: "mic")
                    }
                    .accessibilityIdentifier("microphone-permission-button")

                    Text(viewModel.microphoneStateText)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("microphone-permission-status")
                }

                HStack {
                    Button {
                        viewModel.requestPasteControlAccess()
                    } label: {
                        Label("Allow Paste Control", systemImage: "keyboard")
                    }
                    .accessibilityIdentifier("paste-permission-button")

                    Text(viewModel.pasteControlStateText)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("paste-permission-status")
                }
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("Settings")
                    .font(.headline)

                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 8) {
                    GridRow {
                        Text("Replace")
                            .foregroundStyle(.secondary)
                        TextField("parakeet", text: $viewModel.replacementPattern)
                            .textFieldStyle(.roundedBorder)
                            .accessibilityIdentifier("replacement-pattern-field")
                    }

                    GridRow {
                        Text("With")
                            .foregroundStyle(.secondary)
                        TextField("Canary", text: $viewModel.replacementValue)
                            .textFieldStyle(.roundedBorder)
                            .accessibilityIdentifier("replacement-value-field")
                    }
                }

                Button {
                    viewModel.applyReplacementRule()
                } label: {
                    Label("Apply", systemImage: "checkmark")
                }
                .accessibilityIdentifier("apply-replacement-button")
            }

            VStack(alignment: .leading, spacing: 10) {
                Text("Benchmark")
                    .font(.headline)

                HStack {
                    Button {
                        viewModel.runBenchmark()
                    } label: {
                        Label("Run Benchmark", systemImage: "speedometer")
                    }
                    .accessibilityIdentifier("benchmark-run-button")
                    .disabled(viewModel.selectedModelID.isEmpty || viewModel.benchmarkIsRunning)

                    Text(viewModel.benchmarkStatusText)
                        .foregroundStyle(.secondary)
                        .accessibilityIdentifier("benchmark-status")
                }

                if !viewModel.benchmarkResultsText.isEmpty {
                    Text(viewModel.benchmarkResultsText)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .accessibilityIdentifier("benchmark-results")
                }
            }

            Spacer(minLength: 8)

            HStack {
                Button {
                    viewModel.toggleRecording()
                } label: {
                    Label(
                        viewModel.isRecording ? "Stop" : "Record",
                        systemImage: viewModel.isRecording ? "stop.fill" : "record.circle"
                    )
                    .frame(minWidth: 110)
                }
                .buttonStyle(.borderedProminent)
                .tint(viewModel.isRecording ? .red : .accentColor)
                .accessibilityIdentifier("record-toggle-button")
                .disabled(!viewModel.canRecord)

                Button {
                    viewModel.cancelRecording()
                } label: {
                    Label("Cancel", systemImage: "xmark")
                }
                .accessibilityIdentifier("cancel-recording-button")
                .disabled(!viewModel.isRecording)

                Spacer()
            }

            if !viewModel.lastTranscript.isEmpty {
                Text(viewModel.lastTranscript)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .accessibilityIdentifier("last-transcript")
            }
        }
        .padding(24)
        .onAppear {
            viewModel.loadSelectedModel()
        }
    }
}
