import * as fs from 'node:fs/promises'
import * as os from 'node:os'
import * as path from 'node:path'

import test from 'ava'

import { SpadeBox } from '../index'

async function withTmpDir(fn: (dir: string) => Promise<void>): Promise<void> {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), 'spadebox-'))
  try {
    await fn(dir)
  } finally {
    await fs.rm(dir, { recursive: true })
  }
}

test('write then read round-trips content', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('hello.txt', 'hello world')
    const content = await sb.readFile('hello.txt')
    t.is(content, 'hello world')
  })
})

test('edit_file replaces a string', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('greet.txt', 'hello world')
    await sb.editFile('greet.txt', 'world', 'spadebox')
    const content = await sb.readFile('greet.txt')
    t.is(content, 'hello spadebox')
  })
})

test('edit_file with replace_all replaces all occurrences', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('rep.txt', 'a a a')
    await sb.editFile('rep.txt', 'a', 'b', true)
    const content = await sb.readFile('rep.txt')
    t.is(content, 'b b b')
  })
})

test('read_file throws on missing file', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await t.throwsAsync(() => sb.readFile('nope.txt'), { message: /not found/ })
  })
})

test('grep finds matching lines with file and line number', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('src.ts', 'const x = 1\nconst y = 2\nconst z = 3\n')
    const result = await sb.grep('const y')
    t.regex(result, /src\.ts:2:const y = 2/)
    t.false(result.includes('const x'))
  })
})

test('grep glob restricts search to matching files', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('code.ts', 'const needle = 1\n')
    await sb.writeFile('note.txt', 'const needle = 1\n')
    const result = await sb.grep('needle', '**/*.ts')
    t.true(result.includes('code.ts'))
    t.false(result.includes('note.txt'))
  })
})

test('grep returns no-matches message when nothing found', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('file.txt', 'nothing here\n')
    const result = await sb.grep('xyzzy')
    t.is(result, 'No matches found.')
  })
})

test('grep context_lines includes surrounding lines', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('ctx.txt', 'before\nMATCH\nafter\n')
    const result = await sb.grep('MATCH', undefined, 1)
    t.regex(result, /2:MATCH/)
    t.regex(result, /1-before/)
    t.regex(result, /3-after/)
  })
})

test('path traversal is rejected', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await t.throwsAsync(() => sb.readFile('../etc/passwd'), {
      message: /escape|permission/i,
    })
  })
})

// --- callTool ---

test('callTool dispatches read_file and returns output', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await sb.writeFile('hello.txt', 'hi from callTool')
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'hello.txt' }))
    t.false(result.isError)
    t.is(result.output, 'hi from callTool')
  })
})

test('callTool returns isError=true for tool-level errors (file not found)', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: 'missing.txt' }))
    t.true(result.isError)
    t.regex(result.output, /not found/i)
  })
})

test('callTool throws on unknown tool name (protocol error)', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await t.throwsAsync(() => sb.callTool('no_such_tool', '{}'), { message: /unknown tool/ })
  })
})

test('callTool throws on malformed params JSON (protocol error)', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await t.throwsAsync(() => sb.callTool('read_file', 'not json at all'))
  })
})

test('callTool returns isError=true for sandbox escape attempt', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    const result = await sb.callTool('read_file', JSON.stringify({ path: '../etc/passwd' }))
    t.true(result.isError)
    t.regex(result.output, /escape|permission/i)
  })
})
