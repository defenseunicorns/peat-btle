// Copyright (c) 2025-2026 (r)evolve - Revolve Team LLC
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("maven-publish")
    id("signing")
}

group = "com.defenseunicorns"
version = "0.1.1"

android {
    namespace = "com.defenseunicorns.peat"
    compileSdk = 34

    defaultConfig {
        minSdk = 26  // Wear OS 3 minimum
        targetSdk = 34

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")

        // Configure NDK for native library
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }

        // Build-time configuration for mesh credentials
        // Set via environment variables when building:
        //   PEAT_ENCRYPTION_SECRET=<64-char-hex> ./gradlew assembleRelease
        //   PEAT_MESH_ID=ALPHA ./gradlew assembleRelease
        // Downstream projects can override in their build.gradle.kts:
        //   buildConfigField("String", "PEAT_ENCRYPTION_SECRET", "\"...\"")
        buildConfigField("String", "PEAT_ENCRYPTION_SECRET",
            "\"${System.getenv("PEAT_ENCRYPTION_SECRET") ?: ""}\"")
        buildConfigField("String", "PEAT_MESH_ID",
            "\"${System.getenv("PEAT_MESH_ID") ?: ""}\"")
    }

    buildFeatures {
        buildConfig = true
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

    // Configure publishing variant
    publishing {
        singleVariant("release") {
            withSourcesJar()
        }
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("androidx.annotation:annotation:1.7.1")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.7.3")

    // UniFFI uses JNA for FFI
    implementation("net.java.dev.jna:jna:5.14.0@aar")

    // Testing
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.5")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.1")
}

// Task to build native libraries using Cargo
tasks.register<Exec>("buildNativeLibs") {
    description = "Build native Rust libraries for Android"
    group = "build"

    // peat-btle root is parent of android directory
    val peatBtleRoot = rootProject.projectDir.parentFile
    workingDir = peatBtleRoot

    val ndkPath = System.getenv("ANDROID_NDK_HOME")
        ?: System.getenv("NDK_HOME")
        ?: "${System.getenv("ANDROID_HOME")}/ndk/27.0.12077973"

    environment("ANDROID_NDK_HOME", ndkPath)
    environment("PATH", "$ndkPath/toolchains/llvm/prebuilt/linux-x86_64/bin:${System.getenv("PATH")}")

    commandLine("bash", "-c", """
        set -e
        echo "Building peat-btle native libraries from: $(pwd)"

        # Build for arm64-v8a (modern Android devices)
        echo "Building for aarch64-linux-android (arm64-v8a)..."
        cargo build --release --target aarch64-linux-android --features android
        mkdir -p android/src/main/jniLibs/arm64-v8a
        cp target/aarch64-linux-android/release/libpeat_btle.so android/src/main/jniLibs/arm64-v8a/

        # Build for armeabi-v7a (older devices)
        echo "Building for armv7-linux-androideabi (armeabi-v7a)..."
        cargo build --release --target armv7-linux-androideabi --features android
        mkdir -p android/src/main/jniLibs/armeabi-v7a
        cp target/armv7-linux-androideabi/release/libpeat_btle.so android/src/main/jniLibs/armeabi-v7a/

        # Build for x86_64 (emulators)
        echo "Building for x86_64-linux-android (x86_64)..."
        cargo build --release --target x86_64-linux-android --features android
        mkdir -p android/src/main/jniLibs/x86_64
        cp target/x86_64-linux-android/release/libpeat_btle.so android/src/main/jniLibs/x86_64/

        echo ""
        echo "Native libraries built successfully!"
        echo "  arm64-v8a: android/src/main/jniLibs/arm64-v8a/libpeat_btle.so"
        echo "  armeabi-v7a: android/src/main/jniLibs/armeabi-v7a/libpeat_btle.so"
        echo "  x86_64: android/src/main/jniLibs/x86_64/libpeat_btle.so"
    """.trimIndent())
}

// Task to clean native libraries
tasks.register<Delete>("cleanNativeLibs") {
    description = "Clean native Rust libraries"
    group = "build"

    delete(
        "src/main/jniLibs/arm64-v8a/libpeat_btle.so",
        "src/main/jniLibs/armeabi-v7a/libpeat_btle.so",
        "src/main/jniLibs/x86_64/libpeat_btle.so"
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
                groupId = "com.defenseunicorns"
                artifactId = "peat-btle"
                version = project.version.toString()

                from(components["release"])

                pom {
                    name.set("Peat BLE Android")
                    description.set("Bluetooth Low Energy mesh transport for Peat Protocol - Android library")
                    url.set("https://github.com/defenseunicorns/peat-btle")

                    licenses {
                        license {
                            name.set("Apache License 2.0")
                            url.set("https://www.apache.org/licenses/LICENSE-2.0")
                        }
                    }

                    developers {
                        developer {
                            id.set("defenseunicorns")
                            name.set("Defense Unicorns")
                            email.set("oss@defenseunicorns.com")
                        }
                    }

                    scm {
                        connection.set("scm:git:git://github.com/defenseunicorns/peat-btle.git")
                        developerConnection.set("scm:git:ssh://github.com/defenseunicorns/peat-btle.git")
                        url.set("https://github.com/defenseunicorns/peat-btle")
                    }
                }
            }
        }

        repositories {
            maven {
                name = "GitHubPackages"
                url = uri("https://maven.pkg.github.com/defenseunicorns/peat-btle")
                credentials {
                    username = project.findProperty("gpr.user") as String? ?: System.getenv("GITHUB_ACTOR")
                    password = project.findProperty("gpr.key") as String? ?: System.getenv("GITHUB_TOKEN")
                }
            }

            // Local staging repository for Central Portal bundle
            maven {
                name = "local"
                url = uri(layout.buildDirectory.dir("repo"))
            }
        }
    }

    // Sign all publications
    signing {
        useGpgCmd()
        sign(publishing.publications["release"])
    }
}

// Task to create Maven Central bundle ZIP
tasks.register<Zip>("createMavenCentralBundle") {
    description = "Create ZIP bundle for Maven Central upload"
    group = "publishing"

    dependsOn("publishReleasePublicationToLocalRepository")

    from(layout.buildDirectory.dir("repo"))
    archiveFileName.set("peat-btle-${project.version}-bundle.zip")
    destinationDirectory.set(layout.buildDirectory.dir("bundle"))
}

// Task to publish to Maven Central via Central Portal API
tasks.register<Exec>("publishToMavenCentral") {
    description = "Upload bundle to Maven Central via Sonatype Central Portal"
    group = "publishing"

    dependsOn("createMavenCentralBundle")

    val bundleFile = layout.buildDirectory.file("bundle/peat-btle-${project.version}-bundle.zip")
    val username = project.findProperty("sonatypeUsername") as String? ?: System.getenv("SONATYPE_USERNAME") ?: ""
    val password = project.findProperty("sonatypePassword") as String? ?: System.getenv("SONATYPE_PASSWORD") ?: ""

    doFirst {
        if (username.isEmpty() || password.isEmpty()) {
            throw GradleException("Sonatype credentials not configured. Set sonatypeUsername and sonatypePassword in gradle.properties")
        }
    }

    commandLine("bash", "-c", """
        curl --fail-with-body \
            -u "$username:$password" \
            -F "bundle=@${bundleFile.get().asFile.absolutePath}" \
            "https://central.sonatype.com/api/v1/publisher/upload?publishingType=AUTOMATIC"
    """.trimIndent())
}
