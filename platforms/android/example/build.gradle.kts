plugins {
    alias(libs.plugins.android.application)
}

android {
    namespace = "dev.nativeshell.example"
    compileSdk = 34

    defaultConfig {
        applicationId = "dev.nativeshell.example"
        minSdk = 24
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    sourceSets {
        // The repository's example programs ship as assets: assets/<program>/...
        getByName("main").assets.directories.add("../../../examples")
    }
}

dependencies {
    implementation(project(":native-shell"))
    implementation(libs.androidx.activity)
    implementation(libs.material)

    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.core)
    androidTestImplementation(libs.androidx.test.runner)
    androidTestImplementation(libs.androidx.test.rules)
    androidTestImplementation(libs.androidx.test.junit)
    androidTestImplementation(libs.uiautomator)
}
