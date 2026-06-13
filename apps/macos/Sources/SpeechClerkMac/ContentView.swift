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
                .labelsHidden()
                .frame(maxWidth: .infinity, alignment: .leading)

                Button {
                    viewModel.loadSelectedModel()
                } label: {
                    Label("Load Model", systemImage: "square.and.arrow.down")
                }
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

                    Text(viewModel.microphoneStateText)
                        .foregroundStyle(.secondary)
                }

                HStack {
                    Button {
                        viewModel.requestPasteControlAccess()
                    } label: {
                        Label("Allow Paste Control", systemImage: "keyboard")
                    }

                    Text(viewModel.pasteControlStateText)
                        .foregroundStyle(.secondary)
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
                    }

                    GridRow {
                        Text("With")
                            .foregroundStyle(.secondary)
                        TextField("Canary", text: $viewModel.replacementValue)
                            .textFieldStyle(.roundedBorder)
                    }
                }

                Button {
                    viewModel.applyReplacementRule()
                } label: {
                    Label("Apply", systemImage: "checkmark")
                }
            }

            Spacer(minLength: 8)

            HStack {
                Button {
                    viewModel.toggleRecording()
                } label: {
                    Label(viewModel.isRecording ? "Stop" : "Record", systemImage: viewModel.isRecording ? "stop.fill" : "record.circle")
                        .frame(minWidth: 110)
                }
                .buttonStyle(.borderedProminent)
                .tint(viewModel.isRecording ? .red : .accentColor)
                .disabled(!viewModel.canRecord)

                Button {
                    viewModel.cancelRecording()
                } label: {
                    Label("Cancel", systemImage: "xmark")
                }
                .disabled(!viewModel.isRecording)

                Spacer()
            }

            if !viewModel.lastTranscript.isEmpty {
                Text(viewModel.lastTranscript)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .padding(24)
        .onAppear {
            viewModel.loadSelectedModel()
        }
    }
}
