import { assert, assertEquals, assertMatch, assertRejects } from 'jsr:@std/assert'
import * as fs from 'node:fs/promises'
import { createRequire } from 'node:module'
import * as os from 'node:os'
import * as path from 'node:path'

const require = createRequire(import.meta.url)
const { SpadeBox } = require('../index.js') as typeof import('../index.js')

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
    const sb = new SpadeBox(dir)
    await sb.writeFile('hello.txt', 'hello world')
    const content = await sb.readFile('hello.txt')
    assertEquals(content, 'hello world')
  })
})

Deno.test('edit_file replaces a string', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('greet.txt', 'hello world')
    await sb.editFile('greet.txt', 'world', 'spadebox')
    const content = await sb.readFile('greet.txt')
    assertEquals(content, 'hello spadebox')
  })
})

Deno.test('edit_file with replace_all replaces all occurrences', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('rep.txt', 'a a a')
    await sb.editFile('rep.txt', 'a', 'b', true)
    const content = await sb.readFile('rep.txt')
    assertEquals(content, 'b b b')
  })
})

Deno.test('read_file throws on missing file', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const err = await assertRejects(() => sb.readFile('nope.txt'), Error)
    assertMatch(err.message, /not found/)
  })
})

Deno.test('grep finds matching lines with file and line number', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('src.ts', 'const x = 1\nconst y = 2\nconst z = 3\n')
    const result = await sb.grep('const y')
    assertMatch(result, /src\.ts:2:const y = 2/)
    assert(!result.includes('const x'))
  })
})

Deno.test('grep glob restricts search to matching files', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('code.ts', 'const needle = 1\n')
    await sb.writeFile('note.txt', 'const needle = 1\n')
    const result = await sb.grep('needle', '**/*.ts')
    assert(result.includes('code.ts'))
    assert(!result.includes('note.txt'))
  })
})

Deno.test('grep returns no-matches message when nothing found', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('file.txt', 'nothing here\n')
    const result = await sb.grep('xyzzy')
    assertEquals(result, 'No matches found.')
  })
})

Deno.test('grep context_lines includes surrounding lines', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('ctx.txt', 'before\nMATCH\nafter\n')
    const result = await sb.grep('MATCH', undefined, 1)
    assertMatch(result, /2:MATCH/)
    assertMatch(result, /1-before/)
    assertMatch(result, /3-after/)
  })
})

Deno.test('path traversal is rejected', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const err = await assertRejects(() => sb.readFile('../etc/passwd'), Error)
    assertMatch(err.message, /escape|permission/i)
  })
})

// --- callTool ---

Deno.test('callTool dispatches read_file and returns output', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('hello.txt', 'hi from callTool')
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'hello.txt' }))
    assert(!result.isError)
    assertEquals(result.output, 'hi from callTool')
  })
})

Deno.test('callTool returns isError=true for tool-level errors (file not found)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'missing.txt' }))
    assert(result.isError)
    assertMatch(result.output, /not found/i)
  })
})

Deno.test('callTool throws on unknown tool name (protocol error)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const err = await assertRejects(() => sb.callTool('no_such_tool', '{}'), Error)
    assertMatch(err.message, /unknown tool/)
  })
})

Deno.test('callTool throws on malformed params JSON (protocol error)', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await assertRejects(() => sb.callTool('read_file', 'not json at all'))
  })
})

Deno.test('callTool returns isError=true for sandbox escape attempt', async () => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: '../etc/passwd' }))
    assert(result.isError)
    assertMatch(result.output, /escape|permission/i)
  })
})
