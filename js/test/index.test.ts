import { assert, assertEquals, assertMatch, assertRejects, assertThrows } from '@std/assert'
import * as fs from 'node:fs/promises'
import * as os from 'node:os'
import * as path from 'node:path'
import { SpadeBox } from '@spadebox/spadebox'

async function withTmpDir(fn: (dir: string) => Promise<void>): Promise<void> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'spadebox-'))
  try {
    await fn(dir)
  } finally {
    await fs.rm(dir, { recursive: true })
  }
}

Deno.test('write then read round-trips content', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('hello.txt', 'hello world')
    const content = await sb.readFile('hello.txt')
    assertEquals(content, 'hello world')
  })
})

Deno.test('edit_file replaces a string', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('greet.txt', 'hello world')
    await sb.editFile('greet.txt', 'world', 'spadebox')
    const content = await sb.readFile('greet.txt')
    assertEquals(content, 'hello spadebox')
  })
})

Deno.test('edit_file with replace_all replaces all occurrences', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('rep.txt', 'a a a')
    await sb.editFile('rep.txt', 'a', 'b', true)
    const content = await sb.readFile('rep.txt')
    assertEquals(content, 'b b b')
  })
})

Deno.test('read_file throws on missing file', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    const err = await assertRejects(() => sb.readFile('nope.txt'), Error)
    assertMatch(err.message, /not found/)
  })
})

Deno.test('grep finds matching lines with file and line number', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('src.ts', 'const x = 1\nconst y = 2\nconst z = 3\n')
    const result = await sb.grep('const y')
    assertMatch(result, /src\.ts:2:const y = 2/)
    assert(!result.includes('const x'))
  })
})

Deno.test('grep glob restricts search to matching files', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('code.ts', 'const needle = 1\n')
    await sb.writeFile('note.txt', 'const needle = 1\n')
    const result = await sb.grep('needle', '**/*.ts')
    assert(result.includes('code.ts'))
    assert(!result.includes('note.txt'))
  })
})

Deno.test('grep returns no-matches message when nothing found', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('file.txt', 'nothing here\n')
    const result = await sb.grep('xyzzy')
    assertEquals(result, 'No matches found.')
  })
})

Deno.test('grep context_lines includes surrounding lines', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('ctx.txt', 'before\nMATCH\nafter\n')
    const result = await sb.grep('MATCH', undefined, 1)
    assertMatch(result, /2:MATCH/)
    assertMatch(result, /1-before/)
    assertMatch(result, /3-after/)
  })
})

Deno.test('path traversal is rejected', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    const err = await assertRejects(() => sb.readFile('../etc/passwd'), Error)
    assertMatch(err.message, /escape|permission/i)
  })
})

// --- jsRepl ---

Deno.test('jsRepl evaluates an expression', async () => {
  const sb = new SpadeBox().enableJs()
  const result = await sb.jsRepl('1 + 1')
  assertEquals(result, '2')
})

Deno.test('jsRepl session is persistent across calls', async () => {
  const sb = new SpadeBox().enableJs()
  await sb.jsRepl('let x = 42;')
  const result = await sb.jsRepl('x')
  assertEquals(result, '42')
})

Deno.test('jsRepl throws on JS errors', async () => {
  const sb = new SpadeBox().enableJs()
  const err = await assertRejects(() => sb.jsRepl("throw new Error('oops')"), Error)
  assertMatch(err.message, /JS error/i)
})

// --- exposeJsFunc ---

Deno.test('exposeJsFunc is callable from jsRepl', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('double', ['n'], ({ n }) => (n as number) * 2)
  const result = await sb.jsRepl('double(21)')
  assertEquals(result, '42')
})

Deno.test('exposeJsFunc string return value', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('greet', ['name'], ({ name }) => `hello, ${name}`)
  const result = await sb.jsRepl("greet('world')")
  assertEquals(result, '"hello, world"')
})

Deno.test('exposeJsFunc error surfaces as JS Error', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('boom', [], () => {
    throw new Error('intentional failure')
  })
  const result = await sb.jsRepl('try { boom() } catch(e) { e.message }')
  assertMatch(result, /intentional failure/)
})

Deno.test('exposeJsFunc persists across jsRepl calls', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('add', ['a', 'b'], ({ a, b }) => (a as number) + (b as number))
  await sb.jsRepl('let sum = add(3, 4);')
  const result = await sb.jsRepl('sum')
  assertEquals(result, '7')
})

Deno.test('exposeJsFunc throws if JS is not enabled', () => {
  const sb = new SpadeBox()
  assertThrows(() => sb.exposeJsFunc('f', [], () => null))
})

Deno.test('exposeJsFunc async function resolves promise', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('asyncDouble', ['n'], async ({ n }) => await Promise.resolve((n as number) * 2))
  const result = await sb.jsRepl('asyncDouble(21)')
  assertEquals(result, '42')
})

Deno.test('exposeJsFunc async function with object return', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('asyncObj', ['x'], async ({ x }) => await Promise.resolve({ value: x, doubled: (x as number) * 2 }))
  const result = await sb.jsRepl('asyncObj(5).value')
  assertEquals(result, '5')
})

Deno.test('exposeJsFunc void function (no return value) does not throw', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('noop', [], () => {/* returns nothing — undefined */})
  const result = await sb.jsRepl('noop()')
  assertEquals(result, 'null')
})

Deno.test('exposeJsFunc async void function (no return value) does not throw', async () => {
  const sb = new SpadeBox().enableJs()
  sb.exposeJsFunc('asyncNoop', [], async () => {/* returns nothing — undefined */})
  const result = await sb.jsRepl('asyncNoop()')
  assertEquals(result, 'null')
})

// --- callTool ---

Deno.test('callTool dispatches read_file and returns output', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await sb.writeFile('hello.txt', 'hi from callTool')
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'hello.txt' }))
    assert(!result.isError)
    assertEquals(result.output, 'hi from callTool')
  })
})

Deno.test('callTool returns isError=true for tool-level errors (file not found)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'missing.txt' }))
    assert(result.isError)
    assertMatch(result.output, /not found/i)
  })
})

Deno.test('callTool throws on unknown tool name (protocol error)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    const err = await assertRejects(() => sb.callTool('no_such_tool', '{}'), Error)
    assertMatch(err.message, /unknown tool/)
  })
})

Deno.test('callTool throws on malformed params JSON (protocol error)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    await assertRejects(() => sb.callTool('read_file', 'not json at all'))
  })
})

Deno.test('callTool returns isError=true for sandbox escape attempt', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox().enableFiles(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: '../etc/passwd' }))
    assert(result.isError)
    assertMatch(result.output, /escape|permission/i)
  })
})
