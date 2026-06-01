/**
 * Example agent using any OpenAI-compatible chat completions API.
 *
 * Usage:
 *   deno run --allow-read --allow-write --allow-env --allow-ffi --allow-net \
 *            agent.ts <absolute-sandbox-path>
 *
 * Environment variables:
 *   LLM_BASE_URL  Base URL of the chat completions API (e.g.: https://api.openai.com)
 *   LLM_API_KEY   API key
 *   LLM_MODEL     Model name
 */

import process from 'node:process'
import * as readline from 'node:readline'
import { SpadeBox } from '@spadebox/spadebox'

// --- Configuration ---

const BASE_URL = (Deno.env.get('LLM_BASE_URL') ?? 'http://localhost:8324').replace(/\/$/, '')
const API_KEY = Deno.env.get('LLM_API_KEY') ?? ''
const MODEL = Deno.env.get('LLM_MODEL') ?? 'none'

// --- Colors ---

const RESET = '\x1b[0m'
const BLUE = '\x1b[34m'
const GREEN = '\x1b[32m'
const RED = '\x1b[31m'
const GRAY = '\x1b[90m'
const CYAN = '\x1b[36m'

// --- Validate args ---

const sandboxPath = Deno.args[0]
if (!sandboxPath) {
  console.error('Usage: deno run <perms> agent.ts <absolute-sandbox-path>')
  Deno.exit(1)
}

// --- Setup SpadeBox ---

// [snippet: setup]
const sb = new SpadeBox()
sb.enableFiles(sandboxPath)
  .enableJs()
  .enableHttp()
  .allow('*', ['GET', 'HEAD'])
// [/snippet]

// [snippet: tool-definitions]
const tools = sb.tools().map((t) => ({
  type: 'function' as const,
  function: {
    name: t.name,
    description: t.description,
    parameters: JSON.parse(t.inputSchema),
  },
}))
// [/snippet]

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

// --- API ---

async function chat(messages: Message[]): Promise<AssistantMessage> {
  const res = await fetch(`${BASE_URL}/v1/chat/completions`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${API_KEY}`,
    },
    body: JSON.stringify({ model: MODEL, messages, tools, tool_choice: 'auto' }),
  })
  if (!res.ok) {
    throw new Error(`API error ${res.status}: ${await res.text()}`)
  }
  const data = await res.json()
  return data.choices[0].message as AssistantMessage
}

// --- Agent loop ---

// [snippet: agent-loop]
async function runTurn(messages: Message[]): Promise<void> {
  while (true) {
    const response = await chat(messages)
    messages.push({ role: 'assistant', content: response.content, tool_calls: response.tool_calls })

    if (!response.tool_calls?.length) {
      if (response.content) console.log(`\n${CYAN}Agent:${RESET} ${response.content}\n`)
      return
    }

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
// [/snippet]

// --- Main ---

const SYSTEM_PROMPT = `You are a helpful agent, help the user and use your tools as appropriate.`

const messages: Message[] = [{ role: 'system', content: SYSTEM_PROMPT }]

console.log(`Agent ready. Sandbox: ${sandboxPath}`)
console.log(`Endpoint: ${BASE_URL}, Model: ${MODEL}`)
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
  try { rl.prompt() } catch { /* stdin closed during the last turn */ }
}
