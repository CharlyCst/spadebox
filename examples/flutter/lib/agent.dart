// Agent loop for a SpadeBox-powered AI assistant.
//
// Uses the OpenAI-compatible /v1/chat/completions endpoint so it works with
// any provider — Anthropic, Mistral, a local Ollama instance, or the
// SpadeBox mock server (`just mock-server`).

import 'dart:convert';
import 'package:http/http.dart' as http;
import 'package:spadebox_dart/spadebox.dart';

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

sealed class AgentEvent {}

/// A text block emitted by the assistant (reasoning, commentary).
class AgentThinking extends AgentEvent {
  final String text;
  AgentThinking(this.text);
}

/// The assistant has decided to call a SpadeBox tool.
class AgentToolCall extends AgentEvent {
  final String toolName;
  final Map<String, dynamic> input;
  AgentToolCall(this.toolName, this.input);
}

/// The result returned by a SpadeBox tool.
class AgentToolResult extends AgentEvent {
  final String toolName;
  final bool isError;
  final String output;
  AgentToolResult(this.toolName, {required this.isError, required this.output});
}

/// The agent has finished and produced a final answer.
class AgentDone extends AgentEvent {
  final String text;
  AgentDone(this.text);
}

/// An unrecoverable error (API error, network failure, bad response shape).
class AgentError extends AgentEvent {
  final String message;
  AgentError(this.message);
}

// ---------------------------------------------------------------------------
// Agent loop
// ---------------------------------------------------------------------------

/// Run an agentic loop against any OpenAI-compatible chat completions endpoint.
///
/// Yields [AgentEvent]s in real time as the agent thinks and uses tools.
/// Completes when the model reaches [AgentDone] or [AgentError].
///
/// [sandbox] must already be configured (call `enableFiles`, `enableHttp`,
/// etc.) before passing it here.
Stream<AgentEvent> runAgent({
  required SpadeBox sandbox,
  required String baseUrl,
  required String apiKey,
  required String model,
  required String task,
}) async* {
  // Derive tool definitions from the live sandbox configuration.
  final rawTools = await sandbox.tools();
  final tools = rawTools.map((t) => {
        'type': 'function',
        'function': {
          'name': t.name,
          'description': t.description,
          'parameters': jsonDecode(t.inputSchema) as Map<String, dynamic>,
        },
      }).toList();

  final messages = <Map<String, dynamic>>[
    {
      'role': 'system',
      'content':
          'You are a helpful assistant running on a mobile device. '
          'You have access to a sandboxed file system via tools. '
          'Use them to complete the user\'s task.',
    },
    {'role': 'user', 'content': task},
  ];

  while (true) {
    http.Response response;
    try {
      response = await http.post(
        Uri.parse('$baseUrl/v1/chat/completions'),
        headers: {
          'content-type': 'application/json',
          if (apiKey.isNotEmpty) 'authorization': 'Bearer $apiKey',
        },
        body: jsonEncode({
          'model': model,
          'messages': messages,
          if (tools.isNotEmpty) 'tools': tools,
        }),
      );
    } catch (e) {
      yield AgentError('Network error: $e');
      return;
    }

    if (response.statusCode != 200) {
      yield AgentError('API ${response.statusCode}: ${response.body}');
      return;
    }

    final Map<String, dynamic> body;
    try {
      body = jsonDecode(response.body) as Map<String, dynamic>;
    } catch (e) {
      yield AgentError('Malformed response: $e');
      return;
    }

    final choice = (body['choices'] as List?)?.first as Map<String, dynamic>?;
    if (choice == null) {
      yield AgentError('Empty choices in response');
      return;
    }

    final message = choice['message'] as Map<String, dynamic>;
    final finishReason = choice['finish_reason'] as String? ?? 'stop';

    messages.add(message);

    // Emit any text content from this turn.
    final content = message['content'] as String?;
    if (content != null && content.trim().isNotEmpty) {
      yield AgentThinking(content.trim());
    }

    if (finishReason == 'stop' || finishReason == 'end_turn') {
      yield AgentDone(content?.trim() ?? '');
      return;
    }

    if (finishReason != 'tool_calls') {
      // Unknown finish reason — treat as a completed turn.
      yield AgentDone(content?.trim() ?? '');
      return;
    }

    // ---- Execute tool calls ----
    final toolCalls =
        (message['tool_calls'] as List?)?.cast<Map<String, dynamic>>();
    if (toolCalls == null || toolCalls.isEmpty) {
      yield AgentDone(content?.trim() ?? '');
      return;
    }

    for (final call in toolCalls) {
      final callId = call['id'] as String;
      final fn = call['function'] as Map<String, dynamic>;
      final toolName = fn['name'] as String;
      final argsJson = fn['arguments'] as String;

      final Map<String, dynamic> args;
      try {
        args = jsonDecode(argsJson) as Map<String, dynamic>;
      } catch (_) {
        yield AgentError('Could not parse arguments for $toolName: $argsJson');
        return;
      }

      yield AgentToolCall(toolName, args);

      final SbToolResult result;
      try {
        result = await sandbox.callTool(name: toolName, paramsJson: argsJson);
      } catch (e) {
        yield AgentError('Tool dispatch failed for $toolName: $e');
        return;
      }

      yield AgentToolResult(
        toolName,
        isError: result.isError,
        output: result.output,
      );

      messages.add({
        'role': 'tool',
        'tool_call_id': callId,
        'content': result.output,
      });
    }
  }
}
