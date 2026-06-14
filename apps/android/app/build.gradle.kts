import com.ncorti.ktfmt.gradle.tasks.KtfmtCheckTask
import com.ncorti.ktfmt.gradle.tasks.KtfmtFormatTask
import io.gitlab.arturbosch.detekt.Detekt
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

val speechClerkAbi: String = providers.gradleProperty("speechClerkAbi").orElse("arm64-v8a").get()

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

dependencies { implementation("net.java.dev.jna:jna:5.14.0@aar") }
