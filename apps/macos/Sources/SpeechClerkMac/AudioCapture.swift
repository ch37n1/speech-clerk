import AVFoundation

final class AudioCapture {
    private let engine = AVAudioEngine()

    var isRunning: Bool {
        engine.isRunning
    }

    func start(onSamples: @escaping @Sendable ([Float], UInt32, UInt16) -> Void) throws {
        if engine.isRunning {
            return
        }

        let inputNode = engine.inputNode
        let bus = 0
        let format = inputNode.outputFormat(forBus: bus)
        inputNode.removeTap(onBus: bus)
        inputNode.installTap(onBus: bus, bufferSize: 1_024, format: format) { buffer, _ in
            guard let channelData = buffer.floatChannelData else {
                return
            }

            let frameCount = Int(buffer.frameLength)
            let channelCount = Int(format.channelCount)
            guard frameCount > 0, channelCount > 0 else {
                return
            }

            var interleaved = [Float]()
            interleaved.reserveCapacity(frameCount * channelCount)

            for frameIndex in 0..<frameCount {
                for channelIndex in 0..<channelCount {
                    interleaved.append(channelData[channelIndex][frameIndex])
                }
            }

            onSamples(interleaved, UInt32(format.sampleRate), UInt16(format.channelCount))
        }

        engine.prepare()
        try engine.start()
    }

    func stop() {
        if engine.isRunning {
            engine.inputNode.removeTap(onBus: 0)
            engine.stop()
        }
    }
}
