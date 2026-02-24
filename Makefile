.PHONY: help build test clippy fmt fmt-check doc clean \
        build-android generate-bindings build-aar publish-maven-local verify-jni \
        build-android-demo deploy-android android check-android \
        ci ci-rust ci-android \
        publish-crates publish-maven-central \
        range-macos range-linux

# ============================================
# ECHE-BTLE Build System
# ============================================

# Configuration
ANDROID_SDK ?= $(HOME)/Android/Sdk
ADB ?= $(ANDROID_SDK)/platform-tools/adb
JAVA_HOME ?= $(HOME)/.local/share/mise/installs/java/temurin-17.0.17+10
DEMO_APK ?= ../examples/android-hive-demo/app/build/outputs/apk/debug/app-debug.apk

# Android architectures
ANDROID_TARGETS = arm64-v8a armeabi-v7a x86_64

# ============================================
# Help
# ============================================

help:
	@echo "ECHE-BTLE Build System"
	@echo ""
	@echo "Rust Targets:"
	@echo "  build          - Build library (linux feature)"
	@echo "  test           - Run all tests"
	@echo "  clippy         - Run clippy lints"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check code formatting"
	@echo "  doc            - Generate documentation"
	@echo ""
	@echo "Android Targets:"
	@echo "  build-android      - Build native libs (arm64, armv7, x86_64)"
	@echo "  generate-bindings  - Regenerate Kotlin bindings from UniFFI"
	@echo "  build-aar          - Build AAR package (native + bindings)"
	@echo "  publish-maven-local - Publish AAR to Maven Local"
	@echo "  verify-jni         - Verify JNI symbols match Kotlin declarations"
	@echo ""
	@echo "Demo Targets:"
	@echo "  build-android-demo - Build demo APK"
	@echo "  deploy-android     - Deploy demo to device"
	@echo "  android            - Build and deploy demo"
	@echo ""
	@echo "CI Targets:"
	@echo "  ci             - Run full CI pipeline"
	@echo "  ci-rust        - Run Rust CI checks"
	@echo "  ci-android     - Run Android CI checks"
	@echo ""
	@echo "Release Targets:"
	@echo "  publish-crates       - Publish to crates.io"
	@echo "  publish-maven-central - Publish to Maven Central"
	@echo ""
	@echo "Other:"
	@echo "  clean          - Clean all build artifacts"
	@echo ""

# ============================================
# Rust Targets
# ============================================

build:
	@echo "Building eche-btle (linux)..."
	cargo build --features linux
	@echo "✓ Build complete"

test:
	@echo "Running tests..."
	cargo test --features linux
	@echo "✓ Tests passed"

clippy:
	@echo "Running clippy..."
	cargo clippy --features linux -- -D warnings
	@echo "✓ Clippy passed"

fmt:
	@echo "Formatting code..."
	cargo fmt
	@echo "✓ Formatted"

fmt-check:
	@echo "Checking formatting..."
	cargo fmt --check
	@echo "✓ Formatting OK"

doc:
	@echo "Generating documentation..."
	cargo doc --features linux --no-deps
	@echo "✓ Docs generated: target/doc/eche_btle/index.html"

# ============================================
# Android Targets
# ============================================

build-android:
	@echo "Building native libraries for Android..."
	cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 \
		-o android/src/main/jniLibs \
		build --release --features android
	@echo "✓ Native libraries built:"
	@ls -la android/src/main/jniLibs/*/libeche_btle.so

generate-bindings: build-android
	@echo "Generating Kotlin bindings from UniFFI..."
	uniffi-bindgen generate \
		--library android/src/main/jniLibs/arm64-v8a/libeche_btle.so \
		--language kotlin \
		--out-dir android/src/main/kotlin
	@echo "✓ Kotlin bindings generated: android/src/main/kotlin/uniffi/eche_btle/eche_btle.kt"

build-aar: generate-bindings
	@echo "Building AAR..."
	cd android && ./gradlew assembleRelease --no-configuration-cache
	@echo "✓ AAR built: android/build/outputs/aar/"

publish-maven-local: build-android
	@echo "Publishing to Maven Local..."
	cd android && ./gradlew publishToMavenLocal --no-configuration-cache
	@echo "✓ Published to ~/.m2/repository/com/revolveteam/hive/"

# Verify JNI symbols match Kotlin native method declarations
# Focus on EcheMesh class which has the critical peripheral state methods
verify-jni: build-android
	@echo "Verifying JNI symbols for EcheMesh..."
	@echo ""
	@echo "Extracting EcheMesh native method declarations from Kotlin..."
	@grep "private external fun native" android/src/main/java/com/revolveteam/hive/EcheMesh.kt \
		| sed 's/.*fun \(native[^(]*\).*/\1/' \
		| sort -u > /tmp/kotlin_natives.txt
	@echo "Found $$(wc -l < /tmp/kotlin_natives.txt) EcheMesh native declarations"
	@cat /tmp/kotlin_natives.txt
	@echo ""
	@echo "Extracting EcheMesh JNI symbols from .so..."
	@nm -D android/src/main/jniLibs/arm64-v8a/libeche_btle.so \
		| grep "T Java_com_revolveteam_hive_EcheMesh_native" \
		| sed 's/.*EcheMesh_//' \
		| sed 's/<.*//' \
		| sort -u > /tmp/jni_symbols.txt
	@echo "Found $$(wc -l < /tmp/jni_symbols.txt) EcheMesh JNI symbols"
	@cat /tmp/jni_symbols.txt
	@echo ""
	@echo "Checking for missing implementations..."
	@missing=$$(comm -23 /tmp/kotlin_natives.txt /tmp/jni_symbols.txt); \
	if [ -n "$$missing" ]; then \
		echo ""; \
		echo "❌ ERROR: Missing JNI implementations:"; \
		echo "$$missing"; \
		echo ""; \
		echo "These Kotlin native methods have no corresponding Rust JNI binding."; \
		echo "Add implementations in src/platform/android/jni_bridge.rs"; \
		exit 1; \
	else \
		echo "✓ All EcheMesh native methods have JNI implementations"; \
	fi

# ============================================
# Demo Targets
# ============================================

build-android-demo: build-android
	@echo "Building Android demo APK..."
	JAVA_HOME=$(JAVA_HOME) ../examples/android-hive-demo/gradlew \
		-p ../examples/android-hive-demo assembleDebug
	@echo "✓ APK built: $(DEMO_APK)"

deploy-android:
	@echo "Deploying to Android device..."
	$(ADB) install -r $(DEMO_APK)
	@echo "✓ Deployed to device"

android: build-android-demo deploy-android
	@echo "✓ Build and deploy complete"

check-android:
	@echo "Checking Android compilation..."
	cd android && ./gradlew compileReleaseKotlin --no-configuration-cache
	@echo "✓ Kotlin compilation OK"

# ============================================
# CI Targets
# ============================================

ci: ci-rust ci-android
	@echo ""
	@echo "============================================"
	@echo "✓ Full CI pipeline passed"
	@echo "============================================"

ci-rust: fmt-check clippy test
	@echo "✓ Rust CI passed"

ci-android: build-android verify-jni check-android
	@echo "✓ Android CI passed"

# ============================================
# Release Targets
# ============================================

publish-crates:
	@echo "Publishing to crates.io..."
	cargo publish
	@echo "✓ Published to crates.io"

publish-maven-central: build-android
	@echo "Publishing to Maven Central..."
	cd android && ./gradlew publishToMavenCentral --no-configuration-cache
	@echo "✓ Published to Maven Central"

# ============================================
# Clean
# ============================================

clean:
	@echo "Cleaning build artifacts..."
	cargo clean
	rm -rf android/src/main/jniLibs/*/libeche_btle.so
	rm -rf android/build
	rm -rf ../examples/android-hive-demo/app/build
	@echo "✓ Clean complete"

# ============================================
# Range Test Targets
# ============================================

range-macos:
	@echo "Running macOS range test node..."
	RUST_LOG=info cargo run --features macos --example range_test_node_macos

range-macos-debug:
	@echo "Running macOS range test node (DEBUG - all BLE devices)..."
	RUST_LOG=debug cargo run --features macos --example range_test_node_macos -- --debug

range-linux:
	@echo "Running Linux range test node..."
	RUST_LOG=info cargo run --features linux --example range_test_node
