/// SpadeBox Flutter bindings.
///
/// Before using this package, run the code generator from the `flutter/`
/// directory to produce the Dart–Rust FFI bridge:
///
///   dart pub global activate flutter_rust_bridge_codegen
///   flutter_rust_bridge_codegen generate
///
/// The generated files land in `lib/src/rust/` and are not checked in.
library;

export 'src/rust/frb_generated.dart';
