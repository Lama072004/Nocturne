plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

android {
    namespace = "io.github.lama072004.nocturne"
    compileSdk = 36
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_11.toString()
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "io.github.lama072004.nocturne"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        // API 26 (Android 8): ab hier gibt es adaptive Icons als Vektor, wir brauchen
        // also keine PNG-Icons in fünf Größen. Media3 selbst bräuchte nur API 23.
        minSdk = maxOf(flutter.minSdkVersion, 26)
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
    }

    buildTypes {
        release {
            // TODO: Add your own signing config for the release build.
            // Signing with the debug keys for now, so `flutter run --release` works.
            signingConfig = signingConfigs.getByName("debug")
        }
    }
}

flutter {
    source = "../.."
}

dependencies {
    // Media3 = moderne Android-Medienbibliothek von Google.
    //  - exoplayer: Streaming, Puffern, lückenlose Wiedergabe, Audio-Offload
    //  - session:   MediaSession/MediaLibraryService -> Benachrichtigung,
    //               Sperrbildschirm, Bluetooth, Android Auto, Wear OS
    // Alle Media3-Module müssen dieselbe Version haben.
    val media3 = "1.8.0"
    implementation("androidx.media3:media3-exoplayer:$media3")
    implementation("androidx.media3:media3-session:$media3")
    //  - datasource/database: Cache für Downloads und Streaming
    //  - exoplayer-workmanager: setzt abgebrochene Downloads fort, sobald
    //    die Bedingungen (z. B. WLAN) wieder erfüllt sind
    implementation("androidx.media3:media3-datasource:$media3")
    implementation("androidx.media3:media3-database:$media3")
    implementation("androidx.media3:media3-exoplayer-workmanager:$media3")
    implementation("androidx.core:core-ktx:1.16.0")

    // Nur für Tests auf dem PC (./gradlew :app:testDebugUnitTest), z. B. die
    // Rechnung der Überblendung. Landet nicht in der App.
    testImplementation("junit:junit:4.13.2")
}
