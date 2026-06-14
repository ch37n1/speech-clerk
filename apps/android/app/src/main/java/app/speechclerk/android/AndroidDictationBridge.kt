package app.speechclerk.android

import android.content.Context
import android.system.ErrnoException
import android.system.Os
import app.speechclerk.ffi.DictationConfig
import app.speechclerk.ffi.DictationController
import app.speechclerk.ffi.LanguageContext
import java.io.File

final class AndroidDictationBridge(context: Context) {
    private val appContext = context.applicationContext
    private val controller = DictationController(
        DictationConfig(ModelPackStore.modelPacksDir(appContext).absolutePath)
    )

    init {
        configureOnnxRuntimePath()
    }

    fun loadModel(modelId: String) {
        controller.loadModel(modelId)
    }

    fun setLanguageContext(
        activeKeyboardLanguage: String?,
        platformInputLanguage: String?,
        manualOverride: String?,
    ) {
        controller.setLanguageContext(
            LanguageContext(
                activeKeyboardLanguage,
                platformInputLanguage,
                manualOverride,
            )
        )
    }

    fun startRecording() {
        controller.startRecording()
    }

    fun pushAudio(samples: List<Float>, sampleRateHz: UInt, channels: UShort) {
        controller.pushAudio(samples, sampleRateHz, channels)
    }

    fun stopRecording(): String? = controller.stopRecording()?.text

    fun cancelRecording() {
        controller.cancelRecording()
    }

    private fun configureOnnxRuntimePath() {
        val runtime = File(appContext.applicationInfo.nativeLibraryDir, ONNX_RUNTIME_LIBRARY)
        if (!runtime.isFile) {
            return
        }

        try {
            Os.setenv(ONNX_RUNTIME_ENV, runtime.absolutePath, true)
        } catch (_: ErrnoException) {
            return
        }
    }

    private companion object {
        const val ONNX_RUNTIME_ENV = "ORT_DYLIB_PATH"
        const val ONNX_RUNTIME_LIBRARY = "libonnxruntime.so"
    }
}
