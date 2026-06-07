/// SpadeBox agent — pure Dart CLI.
///
/// Usage:
///   dart run agent.dart <sandbox-path>
///
/// Environment:
///   LLM_BASE_URL  Base URL of the chat completions API (default: http://localhost:8324)
///   LLM_API_KEY   API key
///   LLM_MODEL     Model name (default: none)

import 'dart:convert';
import 'dart:ffi';
import 'dart:io';

import 'package:ffi/ffi.dart';

// ── Native handle type ───────────────────────────────────────────────────────

final class _Sb extends Opaque {}

// ── Library loading ──────────────────────────────────────────────────────────

DynamicLibrary _loadLib() {
  final env = Platform.environment['SPADEBOX_LIB'];
  if (env != null) return DynamicLibrary.open(env);

  // Derive path from script location: examples/dart/ → ../../target/debug/
  final repoRoot = File(Platform.script.toFilePath()).parent.parent.parent;
  final libName = Platform.isLinux
      ? 'libspadebox_dart.so'
      : Platform.isMacOS
          ? 'libspadebox_dart.dylib'
          : 'spadebox_dart.dll';
  final libPath = '${repoRoot.path}/target/debug/$libName';
  try {
    return DynamicLibrary.open(libPath);
  } catch (_) {
    stderr.writeln(
        'Could not load $libPath.\n'
        'Build the library first: cargo build -p spadebox-dart\n'
        'Or set SPADEBOX_LIB to point to the compiled .so/.dylib/.dll.');
    exit(1);
  }
}

final _lib = _loadLib();

// ── FFI function bindings ────────────────────────────────────────────────────

final _sbCreate = _lib.lookupFunction<
    Pointer<_Sb> Function(), Pointer<_Sb> Function()>('sb_create');

final _sbDestroy = _lib.lookupFunction<Void Function(Pointer<_Sb>),
    void Function(Pointer<_Sb>)>('sb_destroy');

final _sbEnableFiles = _lib.lookupFunction<
    Pointer<Char> Function(Pointer<_Sb>, Pointer<Char>),
    Pointer<Char> Function(
        Pointer<_Sb>, Pointer<Char>)>('sb_enable_files');

final _sbEnableHttp = _lib.lookupFunction<Void Function(Pointer<_Sb>),
    void Function(Pointer<_Sb>)>('sb_enable_http');

final _sbToolsJson = _lib.lookupFunction<
    Pointer<Char> Function(Pointer<_Sb>),
    Pointer<Char> Function(Pointer<_Sb>)>('sb_tools_json');

final _sbCallTool = _lib.lookupFunction<
    Pointer<Char> Function(Pointer<_Sb>, Pointer<Char>, Pointer<Char>),
    Pointer<Char> Function(Pointer<_Sb>, Pointer<Char>,
        Pointer<Char>)>('sb_call_tool');

final _sbFreeStr = _lib.lookupFunction<Void Function(Pointer<Char>),
    void Function(Pointer<Char>)>('sb_free_str');

// ── Dart wrapper ─────────────────────────────────────────────────────────────

class SpadeBox {
  final Pointer<_Sb> _handle;

  SpadeBox() : _handle = _sbCreate();

  void enableFiles(String path) {
    final arena = Arena();
    try {
      final errPtr = _sbEnableFiles(
          _handle, path.toNativeUtf8(allocator: arena).cast<Char>());
      if (errPtr != nullptr) {
        final err = errPtr.cast<Utf8>().toDartString();
        _sbFreeStr(errPtr);
        throw StateError('enableFiles failed: $err');
      }
    } finally {
      arena.releaseAll();
    }
  }

  void enableHttp() => _sbEnableHttp(_handle);

  List<Map<String, dynamic>> tools() {
    final ptr = _sbToolsJson(_handle);
    final json = ptr.cast<Utf8>().toDartString();
    _sbFreeStr(ptr);
    return (jsonDecode(json) as List).cast<Map<String, dynamic>>();
  }

  ({bool isError, String output}) callTool(String name, String paramsJson) {
    final arena = Arena();
    try {
      final ptr = _sbCallTool(
        _handle,
        name.toNativeUtf8(allocator: arena).cast<Char>(),
        paramsJson.toNativeUtf8(allocator: arena).cast<Char>(),
      );
      final json = ptr.cast<Utf8>().toDartString();
      _sbFreeStr(ptr);
      final m = jsonDecode(json) as Map<String, dynamic>;
      return (isError: m['isError'] as bool, output: m['output'] as String);
    } finally {
      arena.releaseAll();
    }
  }

  void dispose() => _sbDestroy(_handle);
}

// ── Configuration ────────────────────────────────────────────────────────────

final _baseUrl =
    (Platform.environment['LLM_BASE_URL'] ?? 'http://localhost:8324')
        .trimRight()
        .replaceAll(RegExp(r'/$'), '');
final _apiKey = Platform.environment['LLM_API_KEY'] ?? '';
final _model = Platform.environment['LLM_MODEL'] ?? 'none';

// ── ANSI colors ──────────────────────────────────────────────────────────────

const _reset = '\x1b[0m';
const _blue = '\x1b[34m';
const _green = '\x1b[32m';
const _red = '\x1b[31m';
const _gray = '\x1b[90m';
const _cyan = '\x1b[36m';

// ── LLM call ─────────────────────────────────────────────────────────────────

Future<Map<String, dynamic>> _chat(
  List<Map<String, dynamic>> messages,
  List<Map<String, dynamic>> tools,
) async {
  final client = HttpClient();
  try {
    final uri = Uri.parse('$_baseUrl/v1/chat/completions');
    final request = await client.postUrl(uri);
    request.headers.set('Content-Type', 'application/json');
    if (_apiKey.isNotEmpty) {
      request.headers.set('Authorization', 'Bearer $_apiKey');
    }
    final payloadBytes = utf8.encode(jsonEncode({
      'model': _model,
      'messages': messages,
      'tools': tools,
      'tool_choice': 'auto',
    }));
    request.headers.contentLength = payloadBytes.length;
    request.add(payloadBytes);
    final response = await request.close();
    final body = await response.transform(utf8.decoder).join();
    if (response.statusCode != 200) {
      throw Exception('API error ${response.statusCode}: $body');
    }
    final data = jsonDecode(body) as Map<String, dynamic>;
    return data['choices'][0]['message'] as Map<String, dynamic>;
  } finally {
    client.close();
  }
}

// ── Agent loop ────────────────────────────────────────────────────────────────

Future<void> _runTurn(
  SpadeBox sb,
  List<Map<String, dynamic>> messages,
  List<Map<String, dynamic>> tools,
) async {
  while (true) {
    final response = await _chat(messages, tools);
    messages.add({
      'role': 'assistant',
      'content': response['content'],
      'tool_calls': response['tool_calls'],
    });

    final toolCalls =
        (response['tool_calls'] as List?)?.cast<Map<String, dynamic>>() ?? [];

    if (toolCalls.isEmpty) {
      final content = response['content'] as String?;
      if (content != null && content.isNotEmpty) {
        print('\n${_cyan}Agent:$_reset $content\n');
      }
      return;
    }

    for (final call in toolCalls) {
      final fn = call['function'] as Map<String, dynamic>;
      final name = fn['name'] as String;
      final args = fn['arguments'] as String;
      print('\n${_blue}[call]$_reset $_gray$name($args)$_reset');

      final result = sb.callTool(name, args);
      final tag = result.isError
          ? '$_red[error]$_reset'
          : '$_green[ok]$_reset';
      print('$tag $_gray${result.output}$_reset');

      messages.add({
        'role': 'tool',
        'tool_call_id': call['id'] as String,
        'content': result.output,
      });
    }
  }
}

// ── Entry point ───────────────────────────────────────────────────────────────

void main(List<String> args) async {
  if (args.isEmpty) {
    stderr.writeln('Usage: dart run agent.dart <sandbox-path>');
    exit(1);
  }

  final sandboxPath = args[0];
  final sb = SpadeBox();
  sb.enableFiles(sandboxPath);
  // sb.enableHttp(); // Uncomment to allow the agent to fetch URLs

  final sbTools = sb.tools();
  final tools = sbTools
      .map((t) => {
            'type': 'function',
            'function': {
              'name': t['name'],
              'description': t['description'],
              'parameters': t['inputSchema'],
            },
          })
      .toList();

  print('Agent ready. Sandbox: $sandboxPath');
  print('Endpoint: $_baseUrl, Model: $_model');
  print('Type your request, Ctrl+D to exit.\n');

  const systemPrompt =
      'You are a helpful agent, help the user and use your tools as appropriate.';
  final messages = <Map<String, dynamic>>[
    {'role': 'system', 'content': systemPrompt},
  ];

  while (true) {
    stdout.write('> ');
    final line = stdin.readLineSync();
    if (line == null) break;
    if (line.trim().isEmpty) continue;
    messages.add({'role': 'user', 'content': line});
    try {
      await _runTurn(sb, messages, tools);
    } catch (e) {
      stderr.writeln('Error: $e');
    }
  }

  sb.dispose();
}
