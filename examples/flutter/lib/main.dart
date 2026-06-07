import 'dart:convert';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:path_provider/path_provider.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:spadebox_flutter/spadebox.dart';

import 'agent.dart';

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  // Initialise the Rust–Dart bridge before calling any SpadeBox functions.
  await RustLib.init();
  runApp(const SpadeBoxAgentApp());
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

class SpadeBoxAgentApp extends StatelessWidget {
  const SpadeBoxAgentApp({super.key});

  @override
  Widget build(BuildContext context) => MaterialApp(
        title: 'SpadeBox Agent',
        theme: ThemeData(
          colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xFF4A3880)),
          useMaterial3: true,
        ),
        home: const AgentScreen(),
      );
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

class AgentScreen extends StatefulWidget {
  const AgentScreen({super.key});

  @override
  State<AgentScreen> createState() => _AgentScreenState();
}

class _AgentScreenState extends State<AgentScreen> {
  // Settings
  final _baseUrlCtrl = TextEditingController(text: 'http://localhost:8324');
  final _apiKeyCtrl = TextEditingController();
  final _modelCtrl = TextEditingController(text: 'none');
  String? _sandboxPath;

  // Task
  final _taskCtrl = TextEditingController(
    text: 'List the files available to you, then write a short haiku to haiku.txt.',
  );
  final _scrollCtrl = ScrollController();

  bool _running = false;
  bool _settingsExpanded = false;
  final List<_LogEntry> _log = [];

  @override
  void initState() {
    super.initState();
    _loadSettings();
    getApplicationDocumentsDirectory().then(
      (d) => setState(() => _sandboxPath = d.path),
      onError: (_) {
        // Fallback for environments without XDG dirs (e.g. containers).
        const fallback = '/tmp/spadebox_sandbox';
        Directory(fallback).createSync(recursive: true);
        setState(() => _sandboxPath = fallback);
      },
    );
  }

  Future<void> _loadSettings() async {
    final prefs = await SharedPreferences.getInstance();
    setState(() {
      _baseUrlCtrl.text =
          prefs.getString('baseUrl') ?? 'http://localhost:8324';
      _apiKeyCtrl.text = prefs.getString('apiKey') ?? '';
      _modelCtrl.text = prefs.getString('model') ?? 'none';
    });
  }

  Future<void> _saveSettings() async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString('baseUrl', _baseUrlCtrl.text);
    await prefs.setString('apiKey', _apiKeyCtrl.text);
    await prefs.setString('model', _modelCtrl.text);
  }

  @override
  void dispose() {
    _baseUrlCtrl.dispose();
    _apiKeyCtrl.dispose();
    _modelCtrl.dispose();
    _taskCtrl.dispose();
    _scrollCtrl.dispose();
    super.dispose();
  }

  Future<void> _run() async {
    if (_sandboxPath == null) return;
    await _saveSettings();

    setState(() {
      _running = true;
      _log
        ..clear()
        ..add(_LogEntry(kind: _Kind.user, title: _taskCtrl.text));
    });

    final sandbox = SpadeBox.new_();
    await sandbox.enableFiles(path: _sandboxPath!);

    await for (final event in runAgent(
      sandbox: sandbox,
      baseUrl: _baseUrlCtrl.text.trimRight().replaceAll(RegExp(r'/$'), ''),
      apiKey: _apiKeyCtrl.text.trim(),
      model: _modelCtrl.text.trim(),
      task: _taskCtrl.text,
    )) {
      if (!mounted) break;
      setState(() {
        switch (event) {
          case AgentThinking(:final text):
            _log.add(_LogEntry(kind: _Kind.thinking, title: text));
          case AgentToolCall(:final toolName, :final input):
            _log.add(_LogEntry(
              kind: _Kind.toolCall,
              title: toolName,
              detail: const JsonEncoder.withIndent('  ').convert(input),
            ));
          case AgentToolResult(:final toolName, :final isError, :final output):
            _log.add(_LogEntry(
              kind: isError ? _Kind.toolError : _Kind.toolResult,
              title: toolName,
              detail: output.length > 600
                  ? '${output.substring(0, 600)}…'
                  : output,
            ));
          case AgentDone(:final text):
            _log.add(_LogEntry(kind: _Kind.done, title: text));
          case AgentError(:final message):
            _log.add(_LogEntry(kind: _Kind.error, title: message));
        }
      });
      _scrollToBottom();
    }

    if (mounted) setState(() => _running = false);
  }

  void _scrollToBottom() {
    WidgetsBinding.instance.addPostFrameCallback((_) {
      if (_scrollCtrl.hasClients) {
        _scrollCtrl.animateTo(
          _scrollCtrl.position.maxScrollExtent,
          duration: const Duration(milliseconds: 150),
          curve: Curves.easeOut,
        );
      }
    });
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(
        title: const Text('SpadeBox Agent'),
        backgroundColor: theme.colorScheme.inversePrimary,
        actions: [
          IconButton(
            icon: Icon(
              _settingsExpanded ? Icons.expand_less : Icons.settings_outlined,
            ),
            tooltip: 'Settings',
            onPressed: () =>
                setState(() => _settingsExpanded = !_settingsExpanded),
          ),
        ],
      ),
      body: Column(
        children: [
          // --- Settings panel ---
          AnimatedSize(
            duration: const Duration(milliseconds: 200),
            child: _settingsExpanded
                ? _SettingsPanel(
                    baseUrlCtrl: _baseUrlCtrl,
                    apiKeyCtrl: _apiKeyCtrl,
                    modelCtrl: _modelCtrl,
                    sandboxPath: _sandboxPath,
                    enabled: !_running,
                  )
                : const SizedBox.shrink(),
          ),
          const Divider(height: 1),

          // --- Agent log ---
          Expanded(
            child: _log.isEmpty
                ? Center(
                    child: Text(
                      'Enter a task below and tap Run.',
                      style: theme.textTheme.bodyMedium
                          ?.copyWith(color: theme.colorScheme.outline),
                    ),
                  )
                : ListView.separated(
                    controller: _scrollCtrl,
                    padding: const EdgeInsets.fromLTRB(12, 12, 12, 4),
                    itemCount: _log.length,
                    separatorBuilder: (_, __) => const SizedBox(height: 6),
                    itemBuilder: (_, i) => _LogTile(_log[i]),
                  ),
          ),

          // --- Task input ---
          const Divider(height: 1),
          SafeArea(
            top: false,
            child: Padding(
              padding: const EdgeInsets.fromLTRB(12, 8, 12, 8),
              child: Row(
                crossAxisAlignment: CrossAxisAlignment.end,
                children: [
                  Expanded(
                    child: TextField(
                      controller: _taskCtrl,
                      enabled: !_running,
                      maxLines: 3,
                      minLines: 1,
                      decoration: const InputDecoration(
                        hintText: 'What should the agent do?',
                        border: OutlineInputBorder(),
                        isDense: true,
                        contentPadding:
                            EdgeInsets.symmetric(horizontal: 12, vertical: 10),
                      ),
                    ),
                  ),
                  const SizedBox(width: 8),
                  FilledButton(
                    onPressed: _running ? null : _run,
                    style: FilledButton.styleFrom(
                      minimumSize: const Size(64, 48),
                    ),
                    child: _running
                        ? const SizedBox.square(
                            dimension: 18,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : const Text('Run'),
                  ),
                ],
              ),
            ),
          ),
        ],
      ),
    );
  }
}

// ---------------------------------------------------------------------------
// Settings panel
// ---------------------------------------------------------------------------

class _SettingsPanel extends StatelessWidget {
  const _SettingsPanel({
    required this.baseUrlCtrl,
    required this.apiKeyCtrl,
    required this.modelCtrl,
    required this.sandboxPath,
    required this.enabled,
  });

  final TextEditingController baseUrlCtrl;
  final TextEditingController apiKeyCtrl;
  final TextEditingController modelCtrl;
  final String? sandboxPath;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final outline = Theme.of(context).colorScheme.outline;
    return Padding(
      padding: const EdgeInsets.fromLTRB(12, 10, 12, 10),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.stretch,
        children: [
          _field(baseUrlCtrl, 'API base URL', enabled: enabled),
          const SizedBox(height: 8),
          _field(apiKeyCtrl, 'API key', obscure: true, enabled: enabled),
          const SizedBox(height: 8),
          _field(modelCtrl, 'Model', enabled: enabled),
          if (sandboxPath != null) ...[
            const SizedBox(height: 6),
            Text(
              'Sandbox: $sandboxPath',
              style: TextStyle(fontSize: 11, color: outline),
              overflow: TextOverflow.ellipsis,
            ),
          ],
        ],
      ),
    );
  }

  static Widget _field(
    TextEditingController ctrl,
    String label, {
    bool obscure = false,
    bool enabled = true,
  }) => TextField(
        controller: ctrl,
        obscureText: obscure,
        enabled: enabled,
        decoration: InputDecoration(
          labelText: label,
          border: const OutlineInputBorder(),
          isDense: true,
        ),
      );
}

// ---------------------------------------------------------------------------
// Log model
// ---------------------------------------------------------------------------

enum _Kind { user, thinking, toolCall, toolResult, toolError, done, error }

class _LogEntry {
  const _LogEntry({required this.kind, required this.title, this.detail});
  final _Kind kind;
  final String title;
  final String? detail;
}

// ---------------------------------------------------------------------------
// Log tile
// ---------------------------------------------------------------------------

class _LogTile extends StatelessWidget {
  const _LogTile(this.entry);
  final _LogEntry entry;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);

    final (IconData icon, Color color, String label) = switch (entry.kind) {
      _Kind.user => (
          Icons.person_outline,
          theme.colorScheme.primary,
          'You',
        ),
      _Kind.thinking => (
          Icons.psychology_outlined,
          theme.colorScheme.secondary,
          'Thinking',
        ),
      _Kind.toolCall => (
          Icons.build_outlined,
          const Color(0xFF795500),
          'Tool call',
        ),
      _Kind.toolResult => (
          Icons.check_circle_outline,
          const Color(0xFF2E7D32),
          'Result',
        ),
      _Kind.toolError => (
          Icons.error_outline,
          theme.colorScheme.error,
          'Tool error',
        ),
      _Kind.done => (
          Icons.check_circle,
          theme.colorScheme.primary,
          'Done',
        ),
      _Kind.error => (
          Icons.cancel_outlined,
          theme.colorScheme.error,
          'Error',
        ),
    };

    Color? cardColor;
    if (entry.kind == _Kind.done) {
      cardColor = theme.colorScheme.primaryContainer;
    } else if (entry.kind == _Kind.error || entry.kind == _Kind.toolError) {
      cardColor = theme.colorScheme.errorContainer;
    }

    return Card(
      margin: EdgeInsets.zero,
      color: cardColor,
      child: Padding(
        padding: const EdgeInsets.fromLTRB(12, 8, 12, 10),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.start,
          children: [
            // Header row
            Row(children: [
              Icon(icon, size: 14, color: color),
              const SizedBox(width: 5),
              Text(label,
                  style: theme.textTheme.labelSmall?.copyWith(color: color)),
            ]),
            const SizedBox(height: 4),
            // Body
            Text(entry.title, style: theme.textTheme.bodyMedium),
            // Detail block (tool input/output)
            if (entry.detail != null && entry.detail!.isNotEmpty) ...[
              const SizedBox(height: 6),
              Container(
                width: double.infinity,
                padding: const EdgeInsets.all(8),
                decoration: BoxDecoration(
                  color: theme.colorScheme.surfaceContainerHighest
                      .withValues(alpha: 0.6),
                  borderRadius: BorderRadius.circular(4),
                ),
                child: Text(
                  entry.detail!,
                  style: theme.textTheme.bodySmall?.copyWith(
                    fontFamily: 'monospace',
                    color: theme.colorScheme.onSurfaceVariant,
                  ),
                ),
              ),
            ],
          ],
        ),
      ),
    );
  }
}
