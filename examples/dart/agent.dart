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

import 'package:flutter_rust_bridge/flutter_rust_bridge_for_generated.dart'
    show ExternalLibrary;
import 'package:spadebox_dart/spadebox.dart';

// ── Library loading ──────────────────────────────────────────────────────────

String _libPath() {
  final env = Platform.environment['SPADEBOX_LIB'];
  if (env != null) return env;

  // Derive repo root from script location: examples/dart/ → ../../
  final repoRoot =
      File(Platform.script.toFilePath()).parent.parent.parent.path;
  final name = Platform.isLinux
      ? 'libspadebox_dart.so'
      : Platform.isMacOS
          ? 'libspadebox_dart.dylib'
          : 'spadebox_dart.dll';
  return '$repoRoot/target/debug/$name';
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
      if (tools.isNotEmpty) 'tools': tools,
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

    final content = response['content'] as String?;
    if (content != null && content.trim().isNotEmpty) {
      print('\n${_cyan}Agent:$_reset ${content.trim()}');
    }

    final toolCalls =
        (response['tool_calls'] as List?)?.cast<Map<String, dynamic>>() ?? [];

    if (toolCalls.isEmpty) {
      print('');
      return;
    }

    for (final call in toolCalls) {
      final fn = call['function'] as Map<String, dynamic>;
      final name = fn['name'] as String;
      final args = fn['arguments'] as String;
      print('\n${_blue}[call]$_reset $_gray$name($args)$_reset');

      final result = await sb.callTool(name: name, paramsJson: args);
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

  final libPath = _libPath();
  try {
    await RustLib.init(externalLibrary: ExternalLibrary.open(libPath));
  } catch (e) {
    stderr.writeln('Could not load $libPath\n'
        'Build first: cargo build -p spadebox-dart\n'
        'Or set SPADEBOX_LIB to point to the compiled .so/.dylib/.dll.\n'
        'Details: $e');
    exit(1);
  }

  final sandboxPath = args[0];
  final sb = SpadeBox.new_();
  await sb.enableFiles(path: sandboxPath);
  // await sb.enableHttp(); // Uncomment to allow the agent to fetch URLs

  final rawTools = await sb.tools();
  final tools = rawTools
      .map((t) => {
            'type': 'function',
            'function': {
              'name': t.name,
              'description': t.description,
              'parameters': jsonDecode(t.inputSchema) as Map<String, dynamic>,
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
}
