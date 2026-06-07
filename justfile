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

# Build the Dart native library (libspadebox_dart.so / .dylib / .dll).
# After building, run the CLI example with:
#   dart run examples/dart/agent.dart <sandbox-path>
dart-build:
    cargo build -p spadebox-dart

dart-build-release:
    cargo build --release -p spadebox-dart

# Generate the Dart–Rust FFI bridge (requires flutter_rust_bridge_codegen).
# cargo install flutter_rust_bridge_codegen
dart-codegen:
    cd dart && flutter_rust_bridge_codegen generate

# Build the Dart Android native library for Flutter apps.
# Prerequisites: cargo install cargo-ndk
#                Android NDK pointed to by ANDROID_NDK_HOME or ndk.dir in local.properties
dart-build-android:
    cargo ndk \
        --target aarch64-linux-android \
        --target x86_64-linux-android \
        --output-dir dart/android/src/main/jniLibs \
        -p spadebox-dart \
        build --release

# Start an OpenAI-compatible mock server
mock-server:
    deno task mock-server
