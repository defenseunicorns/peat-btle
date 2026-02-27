// Build script for peat-btle
//
// Currently empty - UniFFI uses proc-macro approach (uniffi::setup_scaffolding!())
// which doesn't require build.rs scaffolding generation.

fn main() {
    // No build-time code generation needed.
    // UniFFI bindings are generated via:
    //   uniffi-bindgen generate --library target/release/libpeat_btle.so --language kotlin
    //   uniffi-bindgen generate --library target/release/libpeat_btle.so --language swift
}
