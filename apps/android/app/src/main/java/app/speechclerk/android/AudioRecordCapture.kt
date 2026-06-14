package app.speechclerk.android

import android.annotation.SuppressLint
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import java.util.concurrent.atomic.AtomicBoolean
import kotlin.math.max

final class AudioRecordCapture {
    private val running = AtomicBoolean(false)
    private var audioRecord: AudioRecord? = null
    private var worker: Thread? = null

    @SuppressLint("MissingPermission")
    fun start(onSamples: (List<Float>, UInt, UShort) -> Unit) {
        if (running.get()) {
            return
        }

        val minBufferSize = AudioRecord.getMinBufferSize(
            SAMPLE_RATE_HZ,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        require(minBufferSize > 0) { "AudioRecord buffer size is unavailable" }

        val bufferSize = max(minBufferSize, SAMPLE_RATE_HZ / 5)
        val recorder = AudioRecord.Builder()
            .setAudioSource(MediaRecorder.AudioSource.VOICE_RECOGNITION)
            .setAudioFormat(
                AudioFormat.Builder()
                    .setSampleRate(SAMPLE_RATE_HZ)
                    .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                    .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
                    .build()
            )
            .setBufferSizeInBytes(bufferSize)
            .build()

        require(recorder.state == AudioRecord.STATE_INITIALIZED) {
            "AudioRecord failed to initialize"
        }

        recorder.startRecording()
        audioRecord = recorder
        running.set(true)
        worker = Thread {
            readLoop(recorder, bufferSize, onSamples)
        }.apply {
            name = "SpeechClerkAudioRecord"
            isDaemon = true
            start()
        }
    }

    fun stop() {
        if (!running.getAndSet(false)) {
            return
        }

        val recorder = audioRecord
        audioRecord = null
        try {
            recorder?.stop()
        } catch (_: IllegalStateException) {
            return
        } finally {
            recorder?.release()
        }

        val thread = worker
        worker = null
        if (thread != null && thread != Thread.currentThread()) {
            try {
                thread.join(STOP_JOIN_MS)
            } catch (_: InterruptedException) {
                Thread.currentThread().interrupt()
            }
        }
    }

    private fun readLoop(
        recorder: AudioRecord,
        bufferSize: Int,
        onSamples: (List<Float>, UInt, UShort) -> Unit,
    ) {
        val pcm = ShortArray(bufferSize / SHORT_BYTES)

        while (running.get()) {
            val read = recorder.read(pcm, 0, pcm.size)
            if (read <= 0) {
                continue
            }

            val samples = ArrayList<Float>(read)
            for (index in 0 until read) {
                samples.add(pcm[index] / PCM_I16_SCALE)
            }
            onSamples(samples, SAMPLE_RATE_HZ.toUInt(), MONO_CHANNELS.toUShort())
        }
    }

    private companion object {
        const val SAMPLE_RATE_HZ = 16_000
        const val MONO_CHANNELS = 1
        const val SHORT_BYTES = 2
        const val PCM_I16_SCALE = 32_768.0f
        const val STOP_JOIN_MS = 500L
    }
}
