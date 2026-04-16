/**
 * Todo list agent using the Mistral chat completions API with tool calling.
 *
 * Usage:
 *   deno run --allow-read --allow-write --allow-env=MISTRAL_API_KEY \
 *            --allow-ffi --allow-net \
 *            example/mistral.ts /absolute/path/to/sandbox
 */

import process from 'node:process'
import * as readline from 'node:readline'
import { SpadeBox } from '@spadebox/spadebox'
import type { SbTool } from '@spadebox/spadebox'

// --- Choose the API endpoint and model ---

// Codestral API, requires a codestral API key
const MISTRAL_API_URL = 'https://codestral.mistral.ai/v1/chat/completions';
const MODEL = 'codestral-latest';

// Mistral API, requires a Mistral API key
// const MISTRAL_API_URL = 'https://api.mistral.ai/v1/chat/completions';
// const MODEL = 'mistral-small-latest';

// --- Colors ---

const RESET  = '\x1b[0m'
const BLUE   = '\x1b[34m'
const GREEN  = '\x1b[32m'
const RED    = '\x1b[31m'
const GRAY   = '\x1b[90m'
const CYAN   = '\x1b[36m'


// --- Validate args and environment ---

const sandboxPath = Deno.args[0]
if (!sandboxPath) {
  console.error('Usage: deno run <perms> example/mistral.ts <absolute-sandbox-path>')
  Deno.exit(1)
}

const apiKey = Deno.env.get('MISTRAL_API_KEY')
if (!apiKey) {
  console.error('Error: MISTRAL_API_KEY environment variable is not set')
  Deno.exit(1)
}

// --- Setup SpadeBox and tools ---

const sb = new SpadeBox()
sb.enableFiles(sandboxPath)
sb.enableHttp().allow('*', ['GET', 'HEAD'])

// Convert SpadeBox tool metadata to the Mistral tool definition format
const tools = sb.tools().map((t: SbTool) => ({
  type: 'function' as const,
  function: {
    name: t.name,
    description: t.description,
    parameters: JSON.parse(t.inputSchema),
  },
}))

// --- Types ---

type Message =
  | { role: 'system'; content: string }
  | { role: 'user'; content: string }
  | { role: 'assistant'; content: string | null; tool_calls?: ToolCall[] }
  | { role: 'tool'; tool_call_id: string; content: string }

interface ToolCall {
  id: string
  type: 'function'
  function: { name: string; arguments: string }
}

interface AssistantMessage {
  role: 'assistant'
  content: string | null
  tool_calls?: ToolCall[]
}

// --- Mistral API ---

async function chat(messages: Message[]): Promise<AssistantMessage> {
  const res = await fetch(MISTRAL_API_URL, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${apiKey}`,
    },
    body: JSON.stringify({ model: MODEL, messages, tools, tool_choice: 'auto' }),
  })
  if (!res.ok) {
    throw new Error(`Mistral API error ${res.status}: ${await res.text()}`)
  }
  const data = await res.json()
  return data.choices[0].message as AssistantMessage
}

// --- Agent loop ---
//
// One user turn may require multiple model + tool-execution rounds before the
// model produces a final text reply. This loop drives that inner cycle.

async function runTurn(messages: Message[]): Promise<void> {
  while (true) {
    const response = await chat(messages)
    messages.push({ role: 'assistant', content: response.content, tool_calls: response.tool_calls })

    // No tool calls → the model is done for this turn
    if (!response.tool_calls || response.tool_calls.length === 0) {
      if (response.content) console.log(`\n${CYAN}Agent:${RESET} ${response.content}\n`)
      return
    }

    // Execute each tool call and feed results back
    for (const call of response.tool_calls) {
      const { name, arguments: args } = call.function
      console.log(`\n${BLUE}[call]${RESET} ${GRAY}${name}(${args})${RESET}`)

      const result = await sb.callTool(name, args)
      const tag = result.isError ? `${RED}[error]${RESET}` : `${GREEN}[ok]${RESET}`
      console.log(`${tag} ${GRAY}${result.output}${RESET}`)

      messages.push({ role: 'tool', tool_call_id: call.id, content: result.output })
    }
  }
}

// --- Main ---

const SYSTEM_PROMPT = `You are a todo list manager. Maintain the user's todo list in a file called todos.md.

Use markdown checkboxes: \`- [ ]\` for pending items, \`- [x]\` for completed ones.
Always read todos.md before making changes. Keep your replies short — just confirm what you did.
You can also fetch URLs to look up information when needed.`

const messages: Message[] = [{ role: 'system', content: SYSTEM_PROMPT }]

console.log(`Todo agent ready. Sandbox: ${sandboxPath}`)
console.log('Type your request, Ctrl+D to exit.\n')

const rl = readline.createInterface({ input: process.stdin, output: process.stdout, prompt: '> ' })
rl.prompt()

for await (const line of rl) {
  if (!line.trim()) continue
  messages.push({ role: 'user', content: line })
  try {
    await runTurn(messages)
  } catch (err) {
    console.error('Error:', err instanceof Error ? err.message : err)
  }
  rl.prompt()
}
