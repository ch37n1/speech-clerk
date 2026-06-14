import com.ncorti.ktfmt.gradle.tasks.KtfmtCheckTask
import com.ncorti.ktfmt.gradle.tasks.KtfmtFormatTask
import io.gitlab.arturbosch.detekt.Detekt
import org.gradle.api.GradleException
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

val speechClerkAbi: String = providers.gradleProperty("speechClerkAbi").orElse("arm64-v8a").get()
val releaseKeystore = providers.environmentVariable("ANDROID_RELEASE_KEYSTORE")
val releaseKeystorePassword = providers.environmentVariable("ANDROID_RELEASE_KEYSTORE_PASSWORD")
val releaseKeyAlias = providers.environmentVariable("ANDROID_RELEASE_KEY_ALIAS")
val releaseKeyPassword = providers.environmentVariable("ANDROID_RELEASE_KEY_PASSWORD")
val releaseSigningValues =
    listOf(releaseKeystore, releaseKeystorePassword, releaseKeyAlias, releaseKeyPassword)
val hasReleaseSigning = releaseSigningValues.all { it.isPresent && it.get().isNotBlank() }

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("com.ncorti.ktfmt.gradle")
    id("io.gitlab.arturbosch.detekt")
}

android {
    namespace = "app.speechclerk.android"
    compileSdk = 35
    buildToolsVersion = "35.0.0"

    defaultConfig {
        applicationId = "app.speechclerk.android"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"

        ndk { abiFilters += speechClerkAbi }
    }

    signingConfigs {
        if (hasReleaseSigning) {
            create("release") {
                storeFile = file(releaseKeystore.get())
                storePassword = releaseKeystorePassword.get()
                keyAlias = releaseKeyAlias.get()
                keyPassword = releaseKeyPassword.get()
            }
        }
    }

    buildTypes {
        getByName("release") {
            signingConfig = signingConfigs.findByName("release")
            isMinifyEnabled = false
        }
    }

    sourceSets {
        getByName("main") {
            java.srcDirs("src/main/java")
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }

    packaging { jniLibs { useLegacyPackaging = true } }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

kotlin { compilerOptions { jvmTarget.set(JvmTarget.JVM_17) } }

ktfmt {
    kotlinLangStyle()
    maxWidth.set(100)
}

detekt {
    toolVersion = "1.23.8"
    buildUponDefaultConfig = true
    allRules = false
    config.setFrom(files("$rootDir/config/detekt/detekt.yml"))
    source.setFrom(
        fileTree("src/main/java") {
            include("**/*.kt")
            exclude("app/speechclerk/ffi/**")
        }
    )
    basePath = projectDir.absolutePath
}

tasks.withType<KtfmtCheckTask>().configureEach { exclude("**/app/speechclerk/ffi/**") }

tasks.withType<KtfmtFormatTask>().configureEach { exclude("**/app/speechclerk/ffi/**") }

tasks.withType<Detekt>().configureEach { jvmTarget = "17" }

tasks.register("validateReleaseSigningEnv") {
    doLast {
        val missing =
            listOf(
                    "ANDROID_RELEASE_KEYSTORE" to releaseKeystore,
                    "ANDROID_RELEASE_KEYSTORE_PASSWORD" to releaseKeystorePassword,
                    "ANDROID_RELEASE_KEY_ALIAS" to releaseKeyAlias,
                    "ANDROID_RELEASE_KEY_PASSWORD" to releaseKeyPassword,
                )
                .filter { (_, provider) -> !provider.isPresent || provider.get().isBlank() }
                .joinToString(", ") { (name, _) -> name }

        if (missing.isNotEmpty()) {
            throw GradleException("Missing Android release signing environment: $missing")
        }
    }
}

tasks
    .matching { it.name == "assembleRelease" }
    .configureEach { dependsOn("validateReleaseSigningEnv") }

dependencies { implementation("net.java.dev.jna:jna:5.14.0@aar") }
