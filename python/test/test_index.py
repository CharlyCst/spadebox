import json
import re
import pytest
from spadebox import SpadeBox


# --- File operations ---

def test_write_then_read_round_trips_content(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('hello.txt', 'hello world')
    content = sb.read_file('hello.txt')
    assert content == 'hello world'


def test_edit_file_replaces_a_string(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('greet.txt', 'hello world')
    sb.edit_file('greet.txt', 'world', 'spadebox')
    content = sb.read_file('greet.txt')
    assert content == 'hello spadebox'


def test_edit_file_with_replace_all_replaces_all_occurrences(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('rep.txt', 'a a a')
    sb.edit_file('rep.txt', 'a', 'b', replace_all=True)
    content = sb.read_file('rep.txt')
    assert content == 'b b b'


def test_read_file_raises_on_missing_file(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    with pytest.raises(RuntimeError, match='not found'):
        sb.read_file('nope.txt')


def test_grep_finds_matching_lines_with_file_and_line_number(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('src.ts', 'const x = 1\nconst y = 2\nconst z = 3\n')
    result = sb.grep('const y')
    assert re.search(r'src\.ts:2:const y = 2', result)
    assert 'const x' not in result


def test_grep_glob_restricts_search_to_matching_files(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('code.ts', 'const needle = 1\n')
    sb.write_file('note.txt', 'const needle = 1\n')
    result = sb.grep('needle', glob='**/*.ts')
    assert 'code.ts' in result
    assert 'note.txt' not in result


def test_grep_returns_no_matches_message_when_nothing_found(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('file.txt', 'nothing here\n')
    result = sb.grep('xyzzy')
    assert result == 'No matches found.'


def test_grep_context_lines_includes_surrounding_lines(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('ctx.txt', 'before\nMATCH\nafter\n')
    result = sb.grep('MATCH', context_lines=1)
    assert re.search(r'2:MATCH', result)
    assert re.search(r'1-before', result)
    assert re.search(r'3-after', result)


def test_path_traversal_is_rejected(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    with pytest.raises(RuntimeError, match=r'escape|permission'):
        sb.read_file('../etc/passwd')


# --- js_repl ---

def test_js_repl_evaluates_an_expression():
    sb = SpadeBox().enable_js()
    result = sb.js_repl('1 + 1')
    assert result == '2'


def test_js_repl_session_is_persistent_across_calls():
    sb = SpadeBox().enable_js()
    sb.js_repl('let x = 42;')
    result = sb.js_repl('x')
    assert result == '42'


def test_js_repl_raises_on_js_errors():
    sb = SpadeBox().enable_js()
    with pytest.raises(RuntimeError, match=r'JS error'):
        sb.js_repl("throw new Error('oops')")


# --- expose_js_func ---

def test_expose_js_func_callable_from_repl():
    sb = SpadeBox().enable_js()
    sb.expose_js_func('double', ['n'], lambda args: args['n'] * 2)
    result = sb.js_repl('double(21)')
    assert result == '42'


def test_expose_js_func_string_return():
    sb = SpadeBox().enable_js()
    sb.expose_js_func('greet', ['name'], lambda args: f"hello, {args['name']}")
    result = sb.js_repl("greet('world')")
    assert result == '"hello, world"'


def test_expose_js_func_error_surfaces_as_js_error():
    sb = SpadeBox().enable_js()

    def boom(args):
        raise ValueError('intentional failure')

    sb.expose_js_func('boom', [], boom)
    result = sb.js_repl("try { boom() } catch(e) { e.message }")
    assert 'intentional failure' in result


def test_expose_js_func_persists_across_repl_calls():
    sb = SpadeBox().enable_js()
    sb.expose_js_func('add', ['a', 'b'], lambda args: args['a'] + args['b'])
    sb.js_repl('let sum = add(3, 4);')
    result = sb.js_repl('sum')
    assert result == '7'


def test_expose_js_func_requires_js_enabled():
    sb = SpadeBox()  # JS not enabled
    with pytest.raises(RuntimeError):
        sb.expose_js_func('f', [], lambda args: None)


# --- call_tool ---

def test_call_tool_dispatches_read_file_and_returns_output(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    sb.write_file('hello.txt', 'hi from call_tool')
    result = sb.call_tool('read_file', json.dumps({'path': 'hello.txt'}))
    assert not result.is_error
    assert result.output == 'hi from call_tool'


def test_call_tool_returns_is_error_for_tool_level_errors(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    result = sb.call_tool('read_file', json.dumps({'path': 'missing.txt'}))
    assert result.is_error
    assert re.search(r'not found', result.output, re.IGNORECASE)


def test_call_tool_raises_on_unknown_tool_name(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    with pytest.raises(ValueError, match='unknown tool'):
        sb.call_tool('no_such_tool', '{}')


def test_call_tool_raises_on_malformed_params_json(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    with pytest.raises((ValueError, RuntimeError)):
        sb.call_tool('read_file', 'not json at all')


def test_call_tool_returns_is_error_for_sandbox_escape_attempt(tmp_path):
    sb = SpadeBox().enable_files(str(tmp_path))
    result = sb.call_tool('read_file', json.dumps({'path': '../etc/passwd'}))
    assert result.is_error
    assert re.search(r'escape|permission', result.output, re.IGNORECASE)
