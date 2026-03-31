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

test('path traversal is rejected', async (t) => {
  await withTmpDir(async (dir) => {
    const sb = new SpadeBox(dir)
    await t.throwsAsync(() => sb.readFile('../etc/passwd'), {
      message: /escape|permission/i,
    })
  })
})
