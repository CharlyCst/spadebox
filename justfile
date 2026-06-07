build:
    cargo build --workspace --exclude spadebox-python
    cd js && deno task build:debug
    cd python && uv run maturin develop

build-release:
    cargo build --release --workspace --exclude spadebox-python
    cd js && deno task build
    cd python && uv run maturin build --release

test:
    # First, we rebuild the bindings
    @just build
    # Then, we run tests
    @just test-only

test-only:
    cargo test
    cd js && deno check && deno task test
    cd python && uv run --no-sync pytest test
    deno task test

lint:
    cargo fmt -- --check
    cargo clippy
    deno lint
    deno fmt --check

# Build the Flutter Android native library.
# Prerequisites: cargo install cargo-ndk
#                Android NDK pointed to by ANDROID_NDK_HOME or ndk.dir in local.properties
flutter-build-android:
    cargo ndk \
        --target aarch64-linux-android \
        --target x86_64-linux-android \
        --output-dir flutter/android/src/main/jniLibs \
        -p spadebox-flutter \
        build --release

# Generate the Dart–Rust FFI bridge (requires flutter_rust_bridge_codegen).
# dart pub global activate flutter_rust_bridge_codegen
flutter-codegen:
    cd flutter && flutter_rust_bridge_codegen generate

# Start an OpenAI-compatible mock server
mock-server:
    deno task mock-server
