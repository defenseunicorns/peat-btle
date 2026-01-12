plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
}

group = "com.revolveteam"
version = "0.0.5"

android {
    namespace = "com.revolveteam.hive"
    compileSdk = 34

    defaultConfig {
        minSdk = 26  // Wear OS 3 minimum
        targetSdk = 34

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")

        // Configure NDK for native library
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro"
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }

    // Source sets to include native libraries
    sourceSets {
        getByName("main") {
            jniLibs.srcDirs("src/main/jniLibs")
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.annotation:annotation:1.7.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // Testing
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
}

// Task to build native libraries using Cargo
tasks.register<Exec>("buildNativeLibs") {
    description = "Build native Rust libraries for Android"
    group = "build"

    // hive-btle root is parent of android directory
    val hiveBtleRoot = rootProject.projectDir.parentFile
    workingDir = hiveBtleRoot

    val ndkPath = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("NDK_HOME")
        ?: "${System.getenv("ANDROID_HOME")}/ndk/27.0.12077973"

    environment("ANDROID_NDK_HOME", ndkPath)
    environment("PATH", "$ndkPath/toolchains/llvm/prebuilt/linux-x86_64/bin:${System.getenv("PATH")}")

    commandLine("bash", "-c", """
        set -e
        echo "Building hive-btle native libraries from: $(pwd)"

        # Build for arm64-v8a (modern Android devices)
        echo "Building for aarch64-linux-android (arm64-v8a)..."
        cargo build --release --target aarch64-linux-android --features android
        mkdir -p android/src/main/jniLibs/arm64-v8a
        cp target/aarch64-linux-android/release/libhive_btle.so android/src/main/jniLibs/arm64-v8a/

        # Build for armeabi-v7a (older devices)
        echo "Building for armv7-linux-androideabi (armeabi-v7a)..."
        cargo build --release --target armv7-linux-androideabi --features android
        mkdir -p android/src/main/jniLibs/armeabi-v7a
        cp target/armv7-linux-androideabi/release/libhive_btle.so android/src/main/jniLibs/armeabi-v7a/

        echo ""
        echo "Native libraries built successfully!"
        echo "  arm64-v8a: android/src/main/jniLibs/arm64-v8a/libhive_btle.so"
        echo "  armeabi-v7a: android/src/main/jniLibs/armeabi-v7a/libhive_btle.so"
    """.trimIndent())
}

// Task to clean native libraries
tasks.register<Delete>("cleanNativeLibs") {
    description = "Clean native Rust libraries"
    group = "build"

    delete(
        "src/main/jniLibs/arm64-v8a/libhive_btle.so",
        "src/main/jniLibs/armeabi-v7a/libhive_btle.so"
    )
}

// Combined task: build native libs + assemble AAR
tasks.register("buildAar") {
    description = "Build native libraries and assemble AAR"
    group = "build"

    dependsOn("buildNativeLibs")
    finalizedBy("assembleRelease")
}

// Task to publish to local Maven for testing
tasks.register("publishLocal") {
    description = "Build and publish AAR to local Maven repository (~/.m2)"
    group = "publishing"

    dependsOn("buildNativeLibs")
    finalizedBy("publishToMavenLocal")
}

// Publishing configuration
afterEvaluate {
    publishing {
        publications {
            register<MavenPublication>("release") {
                groupId = "com.revolveteam"
                artifactId = "hive"
                version = project.version.toString()

                from(components["release"])

                pom {
                    name.set("HIVE Android")
                    description.set("Bluetooth Low Energy mesh transport for HIVE Protocol - Android library by Revolve Team")
                    url.set("https://github.com/Ascent-Integrated-Tech/hive-btle")

                    licenses {
                        license {
                            name.set("Apache License 2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0")
                        }
                    }

                    developers {
                        developer {
                            id.set("revolve")
                            name.set("Revolve Team")
                            email.set("team@revolve.tech")
                        }
                    }

                    scm {
                        connection.set("scm:git:git://github.com/Ascent-Integrated-Tech/hive-btle.git")
                        developerConnection.set("scm:git:ssh://github.com/Ascent-Integrated-Tech/hive-btle.git")
                        url.set("https://github.com/Ascent-Integrated-Tech/hive-btle")
                    }
                }
            }
        }

        repositories {
            maven {
                name = "GitHubPackages"
                url = uri("https://maven.pkg.github.com/Ascent-Integrated-Tech/hive-btle")
                credentials {
                    username = project.findProperty("gpr.user") as String? ?: System.getenv("GITHUB_ACTOR")
                    password = project.findProperty("gpr.key") as String? ?: System.getenv("GITHUB_TOKEN")
                }
            }

            // Local Maven repository for testing
            maven {
                name = "local"
                url = uri(layout.buildDirectory.dir("repo"))
            }
        }
    }
}
