import { join } from '@std/path'
import { start } from '../mock_openai_api/mod.ts'

const RUNNER_DIR = import.meta.dirname!
const REPO_ROOT = join(RUNNER_DIR, '..', '..')
const JS_EXAMPLE_DIR = join(REPO_ROOT, 'examples', 'js')
const PYTHON_EXAMPLE_DIR = join(REPO_ROOT, 'examples', 'python')
const TIMEOUT_MS = 30_000

export interface ExampleResult {
  name: string
  scenario: string
  pass: boolean
  output: string
  error?: string
}

async function runAgent(
  name: string,
  scenario: string,
  cmd: string[],
  cwd: string,
  port: number,
): Promise<ExampleResult> {
  const env = { ...Deno.env.toObject(), LLM_BASE_URL: `http://localhost:${port}` }

  const child = new Deno.Command(cmd[0], {
    args: cmd.slice(1),
    cwd,
    stdin: 'piped',
    stdout: 'piped',
    stderr: 'piped',
    env,
  }).spawn()

  // Send the scenario ID as the user message, then close stdin (simulates Ctrl+D).
  // The agent finishes the turn, reads EOF on the next prompt, and exits cleanly.
  const writer = child.stdin.getWriter()
  await writer.write(new TextEncoder().encode(scenario + '\n'))
  await writer.close()

  const timer = setTimeout(() => child.kill('SIGKILL'), TIMEOUT_MS)
  const { code, stdout, stderr } = await child.output()
  clearTimeout(timer)

  const output = new TextDecoder().decode(stdout)
  const errText = new TextDecoder().decode(stderr).trim()

  const pass = code === 0 && output.includes('Agent:')
  const error = code !== 0
    ? `exit ${code}: ${errText.slice(0, 300)}`
    : !output.includes('Agent:')
    ? 'no Agent: response in output'
    : undefined

  return { name, scenario, pass, output: output.trim(), error }
}

function runRust(scenario: string, sandbox: string, port: number): Promise<ExampleResult> {
  return runAgent('rust', scenario, ['cargo', 'run', '--quiet', '-p', 'example', '--example', 'agent', '--', sandbox], REPO_ROOT, port)
}

function runJs(scenario: string, sandbox: string, port: number): Promise<ExampleResult> {
  return runAgent('js', scenario, ['deno', 'run', '-P', 'agent.ts', sandbox], JS_EXAMPLE_DIR, port)
}

function runPython(scenario: string, sandbox: string, port: number): Promise<ExampleResult> {
  return runAgent('python', scenario, ['uv', 'run', 'agent.py', sandbox], PYTHON_EXAMPLE_DIR, port)
}

interface ExampleTest {
  name: string
  scenario: string
  runner: (scenario: string, sandbox: string, port: number) => Promise<ExampleResult>
}

const TESTS: ExampleTest[] = [
  { name: 'rust', scenario: 'read_file', runner: runRust },
  { name: 'rust', scenario: 'fetch', runner: runRust },
  { name: 'js', scenario: 'read_file', runner: runJs },
  { name: 'js', scenario: 'fetch', runner: runJs },
  { name: 'python', scenario: 'read_file', runner: runPython },
  { name: 'python', scenario: 'fetch', runner: runPython },
]

export async function runExamples(): Promise<ExampleResult[]> {
  const sandbox = await Deno.makeTempDir()
  await Deno.writeTextFile(join(sandbox, 'hello.txt'), 'hello from sandbox')

  const server = await start()

  try {
    return await Promise.all(TESTS.map(({ runner, scenario }) => runner(scenario, sandbox, server.port)))
  } finally {
    await server.shutdown()
    await Deno.remove(sandbox, { recursive: true })
  }
}
