package app.speechclerk.android

import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodSubtype

object LanguageTags {
    fun fromSubtype(subtype: InputMethodSubtype?): String? {
        if (subtype == null) {
            return null
        }

        return clean(subtype.languageTag)
    }

    fun fromEditorInfo(editorInfo: EditorInfo?): String? {
        if (editorInfo == null) {
            return null
        }

        val locales = editorInfo.hintLocales ?: return null
        if (locales.isEmpty) {
            return null
        }
        return clean(locales[0].toLanguageTag())
    }

    fun clean(value: String?): String? {
        val normalized = value?.trim()?.replace('_', '-')?.takeIf(String::isNotEmpty)
        return normalized
    }
}
