import { dirname, fromFileUrl, join } from 'jsr:@std/path'

const RUNNER_DIR = dirname(fromFileUrl(import.meta.url))
const JS_EXEC_DIR = join(RUNNER_DIR, '..', 'js_exec')
const REPO_ROOT = join(RUNNER_DIR, '..', '..')
const CLI = join(REPO_ROOT, 'target', 'debug', 'spadebox-cli')

async function build(): Promise<void> {
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

interface Result {
  file: string
  pass: boolean
  expected: string
  actual: string
  error?: string
}

async function runTest(file: string): Promise<Result> {
  const source = await Deno.readTextFile(join(JS_EXEC_DIR, file))
  const expected = parseExpected(source)
  if (expected === null) {
    return { file, pass: false, expected: '', actual: '', error: 'no // Output: block' }
  }

  const { code, stdout, stderr } = await new Deno.Command(CLI, {
    args: ['run', 'js_exec', JSON.stringify({ path: file })],
    cwd: JS_EXEC_DIR,
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

async function main() {
  console.log('Building spadebox-cli...')
  await build()

  const files: string[] = []
  for await (const e of Deno.readDir(JS_EXEC_DIR)) {
    if (e.isFile && e.name.endsWith('.js')) files.push(e.name)
  }
  files.sort()

  if (files.length === 0) {
    console.log('No test files found in tests/js_exec/')
    Deno.exit(0)
  }

  console.log(`\nRunning ${files.length} test(s)...\n`)

  const results = await Promise.all(files.map(runTest))
  let passed = 0
  let failed = 0

  for (const r of results) {
    if (r.pass) {
      console.log(`  ✓ ${r.file}`)
      passed++
    } else {
      console.log(`  ✗ ${r.file}${r.error ? ` (${r.error})` : ''}`)
      console.log(`    expected: ${JSON.stringify(r.expected)}`)
      console.log(`    actual:   ${JSON.stringify(r.actual)}`)
      failed++
    }
  }

  console.log(`\n${passed} passed, ${failed} failed`)
  if (failed > 0) Deno.exit(1)
}

await main()
