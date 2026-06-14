package app.speechclerk.android

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.inputmethodservice.InputMethodService
import android.os.Handler
import android.os.Looper
import android.view.Gravity
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.view.inputmethod.InputMethodSubtype
import android.widget.Button
import android.widget.LinearLayout
import android.widget.TextView

class SpeechClerkImeService : InputMethodService() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private val audioCapture = AudioRecordCapture()

    private lateinit var bridge: AndroidDictationBridge
    private lateinit var statusView: TextView
    private lateinit var micButton: Button

    private var isRecording = false
    private var modelLoaded = false
    private var activeInputSubtype: InputMethodSubtype? = null

    override fun onCreate() {
        super.onCreate()
        bridge = AndroidDictationBridge(this)
    }

    override fun onCreateInputView(): View = LinearLayout(this).apply {
        orientation = LinearLayout.VERTICAL
        gravity = Gravity.CENTER
        setPadding(dp(12), dp(10), dp(12), dp(10))
        setBackgroundColor(getColor(R.color.ime_background))

        statusView = TextView(context).apply {
            gravity = Gravity.CENTER
            setTextColor(getColor(R.color.ime_muted_text))
            textSize = 14f
            text = getString(R.string.status_idle)
            contentDescription = text
        }
        addView(
            statusView,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                LinearLayout.LayoutParams.WRAP_CONTENT,
            )
        )

        micButton = Button(context).apply {
            minHeight = dp(52)
            setAllCaps(false)
            text = getString(R.string.mic_button_label)
            contentDescription = getString(R.string.mic_button_accessibility)
            setOnClickListener { toggleRecording() }
        }
        addView(
            micButton,
            LinearLayout.LayoutParams(
                LinearLayout.LayoutParams.MATCH_PARENT,
                dp(58),
            ).apply {
                topMargin = dp(8)
            }
        )
        updateMicButton()
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        setLanguageContext(info)
        ensureModelLoaded()
    }

    override fun onFinishInput() {
        if (isRecording) {
            cancelRecording()
        }
        super.onFinishInput()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        if (isRecording) {
            cancelRecording()
        }
        super.onFinishInputView(finishingInput)
    }

    override fun onCurrentInputMethodSubtypeChanged(newSubtype: InputMethodSubtype?) {
        super.onCurrentInputMethodSubtypeChanged(newSubtype)
        activeInputSubtype = newSubtype
        setLanguageContext(currentInputEditorInfo)
    }

    override fun onDestroy() {
        if (isRecording || audioCapture.isRunning()) {
            cancelRecording()
        }
        super.onDestroy()
    }

    private fun toggleRecording() {
        if (isRecording) {
            stopRecording()
        } else {
            startRecording()
        }
    }

    private fun startRecording() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            startActivity(
                Intent(this, PermissionActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            )
            setStatus(getString(R.string.status_mic_permission))
            return
        }

        ensureModelLoaded()
        if (!modelLoaded) {
            setStatus(getString(R.string.status_no_model))
            return
        }

        setLanguageContext(currentInputEditorInfo)

        try {
            bridge.startRecording()
            audioCapture.start { samples, sampleRateHz, channels ->
                try {
                    bridge.pushAudio(samples, sampleRateHz, channels)
                } catch (_: Exception) {
                    postCaptureFailure()
                }
            }
        } catch (_: Exception) {
            audioCapture.stop()
            try {
                bridge.cancelRecording()
            } catch (_: Exception) {
            }
            setStatus(getString(R.string.status_error))
            return
        }

        isRecording = true
        updateMicButton()
        setStatus(getString(R.string.status_recording))
    }

    private fun stopRecording() {
        audioCapture.stop()
        setStatus(getString(R.string.status_transcribing))

        val transcript = try {
            bridge.stopRecording()
        } catch (_: Exception) {
            isRecording = false
            updateMicButton()
            setStatus(getString(R.string.status_error))
            return
        }

        isRecording = false
        updateMicButton()

        val text = transcript?.trim()
        if (text.isNullOrEmpty()) {
            setStatus(getString(R.string.status_no_audio))
            return
        }

        if (currentInputConnection?.commitText(text, 1) == true) {
            setStatus(getString(R.string.status_inserted))
        } else {
            setStatus(getString(R.string.status_error))
        }
    }

    private fun cancelRecording() {
        audioCapture.stop()
        try {
            bridge.cancelRecording()
        } catch (_: Exception) {
            return
        } finally {
            isRecording = false
            updateMicButton()
        }
    }

    private fun ensureModelLoaded() {
        if (modelLoaded) {
            return
        }

        val modelId = ModelPackStore.defaultModelId(this)
        if (modelId == null) {
            modelLoaded = false
            setStatus(getString(R.string.status_no_model))
            return
        }

        try {
            bridge.loadModel(modelId)
        } catch (_: Exception) {
            modelLoaded = false
            setStatus(getString(R.string.status_model_error))
            return
        }

        modelLoaded = true
        setStatus(getString(R.string.status_model_loaded, modelId))
    }

    private fun setLanguageContext(editorInfo: EditorInfo?) {
        bridge.setLanguageContext(
            activeKeyboardLanguage = LanguageTags.fromSubtype(currentInputSubtype()),
            platformInputLanguage = LanguageTags.fromEditorInfo(editorInfo),
            manualOverride = ModelPackStore.manualLanguageOverride(this),
        )
    }

    @Suppress("DEPRECATION")
    private fun currentInputSubtype(): InputMethodSubtype? =
        activeInputSubtype
            ?: getSystemService(InputMethodManager::class.java).currentInputMethodSubtype

    private fun postCaptureFailure() {
        mainHandler.post {
            if (isRecording) {
                audioCapture.stop()
                try {
                    bridge.cancelRecording()
                } catch (_: Exception) {
                } finally {
                    isRecording = false
                    updateMicButton()
                }
                setStatus(getString(R.string.status_error))
            }
        }
    }

    private fun updateMicButton() {
        if (!this::micButton.isInitialized) {
            return
        }

        micButton.text = if (isRecording) {
            getString(R.string.stop_button_label)
        } else {
            getString(R.string.mic_button_label)
        }
        micButton.contentDescription = if (isRecording) {
            getString(R.string.stop_button_accessibility)
        } else {
            getString(R.string.mic_button_accessibility)
        }
        micButton.backgroundTintList = ColorStateList.valueOf(
            getColor(
                if (isRecording) {
                    R.color.ime_button_recording
                } else {
                    R.color.ime_button
                }
            )
        )
        micButton.setTextColor(getColor(R.color.ime_button_text))
    }

    private fun setStatus(value: String) {
        if (this::statusView.isInitialized) {
            statusView.text = value
            statusView.contentDescription = value
        }
    }

    private fun dp(value: Int): Int =
        (value * resources.displayMetrics.density).toInt()
}
