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

function runRust(pkg: string, example: string) {
  return (scenario: string, sandbox: string, port: number): Promise<ExampleResult> =>
    runAgent(
      `rust/${pkg}/${example}`,
      scenario,
      ['cargo', 'run', '--quiet', '-p', pkg, '--example', example, '--', sandbox],
      REPO_ROOT,
      port,
    )
}

function runJs(file: string) {
  return (scenario: string, sandbox: string, port: number): Promise<ExampleResult> =>
    runAgent(`js/${file}`, scenario, ['deno', 'run', '-P', file, sandbox], JS_EXAMPLE_DIR, port)
}

function runPython(file: string) {
  return (scenario: string, sandbox: string, port: number): Promise<ExampleResult> =>
    runAgent(`python/${file}`, scenario, ['uv', 'run', file, sandbox], PYTHON_EXAMPLE_DIR, port)
}

interface ExampleTest {
  name: string
  scenario: string
  runner: (scenario: string, sandbox: string, port: number) => Promise<ExampleResult>
}

const TESTS: ExampleTest[] = [
  { name: 'rust/example/agent', scenario: 'read_file', runner: runRust('example', 'agent') },
  { name: 'rust/example/agent', scenario: 'fetch', runner: runRust('example', 'agent') },
  { name: 'js/agent.ts', scenario: 'read_file', runner: runJs('agent.ts') },
  { name: 'js/agent.ts', scenario: 'fetch', runner: runJs('agent.ts') },
  { name: 'js/subagent.ts', scenario: 'subagent', runner: runJs('subagent.ts') },
  { name: 'python/agent.py', scenario: 'read_file', runner: runPython('agent.py') },
  { name: 'python/agent.py', scenario: 'fetch', runner: runPython('agent.py') },
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
