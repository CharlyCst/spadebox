import { join } from '@std/path'

const RUNNER_DIR = import.meta.dirname!
const JS_EXEC_DIR = join(RUNNER_DIR, '..', 'js_exec')
const REPO_ROOT = join(RUNNER_DIR, '..', '..')
const CLI = join(REPO_ROOT, 'target', 'debug', 'spadebox-cli')

export interface JsRuntimeResult {
  file: string
  pass: boolean
  expected: string
  actual: string
  error?: string
}

async function buildCli(): Promise<void> {
  const { code } = await new Deno.Command('cargo', {
    args: ['build', '--package', 'spadebox-cli'],
    cwd: REPO_ROOT,
    stdout: 'inherit',
    stderr: 'inherit',
  }).output()
  if (code !== 0) throw new Error('cargo build failed')
}

// Parses the expected output from the top-level // Output: block.
// Each "// <line>" becomes a line of expected output; a bare "//" is an empty line.
// Parsing stops at the first line that is neither a comment nor blank.
function parseExpected(source: string): string | null {
  const lines = source.split('\n')
  const output: string[] = []
  let inOutput = false
  for (const line of lines) {
    if (!inOutput) {
      if (line.trimEnd() === '// Output:') inOutput = true
    } else {
      if (line.startsWith('// ')) output.push(line.slice(3))
      else if (line === '//') output.push('')
      else break
    }
  }
  return inOutput ? output.join('\n') : null
}

async function runTest(file: string, tempDir: string): Promise<JsRuntimeResult> {
  const source = await Deno.readTextFile(join(JS_EXEC_DIR, file))
  const expected = parseExpected(source)
  if (expected === null) {
    return { file, pass: false, expected: '', actual: '', error: 'no // Output: block' }
  }

  const { code, stdout, stderr } = await new Deno.Command(CLI, {
    args: ['--root', tempDir, 'run', 'js_exec', JSON.stringify({ path: file })],
    stdout: 'piped',
    stderr: 'piped',
  }).output()

  const actual = new TextDecoder().decode(stdout)
  if (code !== 0) {
    const errText = new TextDecoder().decode(stderr).trim()
    return { file, pass: false, expected, actual: errText, error: `exit ${code}` }
  }
  return { file, pass: actual === expected, expected, actual }
}

export async function runJsRuntime(): Promise<JsRuntimeResult[]> {
  console.log('Building spadebox-cli...')
  await buildCli()

  const files: string[] = []
  for await (const e of Deno.readDir(JS_EXEC_DIR)) {
    if (e.isFile && e.name.endsWith('.js')) files.push(e.name)
  }
  files.sort()

  if (files.length === 0) return []

  const tempDir = await Deno.makeTempDir()
  await Promise.all(files.map((f) => Deno.copyFile(join(JS_EXEC_DIR, f), join(tempDir, f))))

  try {
    return await Promise.all(files.map((f) => runTest(f, tempDir)))
  } finally {
    await Deno.remove(tempDir, { recursive: true })
  }
}
