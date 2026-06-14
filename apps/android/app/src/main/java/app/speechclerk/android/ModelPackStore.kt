package app.speechclerk.android

import android.content.Context
import java.io.File
import org.json.JSONObject

object ModelPackStore {
    fun modelPacksDir(context: Context): File =
        File(context.filesDir, MODEL_PACKS_DIR).also(File::mkdirs)

    fun installedModelIds(context: Context): List<String> {
        val root = modelPacksDir(context)
        return root
            .listFiles()
            ?.asSequence()
            ?.filter(File::isDirectory)
            ?.mapNotNull(::manifestModelId)
            ?.sorted()
            ?.toList() ?: emptyList()
    }

    fun defaultModelId(context: Context): String? = installedModelIds(context).firstOrNull()

    fun manualLanguageOverride(context: Context): String? =
        context
            .getSharedPreferences(PREFERENCES_NAME, Context.MODE_PRIVATE)
            .getString(MANUAL_LANGUAGE_KEY, null)
            ?.let(LanguageTags::clean)

    private fun manifestModelId(packDir: File): String? {
        val manifest = File(packDir, MANIFEST_FILE_NAME)
        if (!manifest.isFile) {
            return null
        }

        return try {
            JSONObject(manifest.readText()).optString("modelId").takeIf(String::isNotEmpty)
        } catch (_: Exception) {
            null
        }
    }

    private const val MODEL_PACKS_DIR = "ModelPacks"
    private const val MANIFEST_FILE_NAME = "manifest.json"
    private const val PREFERENCES_NAME = "speech_clerk_android"
    private const val MANUAL_LANGUAGE_KEY = "manual_language"
}
