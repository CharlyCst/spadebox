/**
 * Example demonstrating synchronous subagents via the JS runtime.
 *
 * The main agent has access to a global `subagent(prompt)` function inside the
 * JS REPL. Calling it spins up a fresh SpadeBox agent that runs to completion
 * and returns the final assistant response as a string — all synchronously from
 * the Boa JS runtime's perspective, thanks to the threadsafe-function bridge in
 * exposeJsFunc.
 *
 * Usage:
 *   deno run --allow-read --allow-write --allow-env --allow-ffi --allow-net \
 *            subagent.ts <absolute-sandbox-path>
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
const YELLOW = '\x1b[33m'

// --- Validate args ---

const sandboxPath = Deno.args[0]
if (!sandboxPath) {
  console.error('Usage: deno run <perms> subagent.ts <absolute-sandbox-path>')
  Deno.exit(1)
}

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

async function chat(messages: Message[], tools: unknown[]): Promise<AssistantMessage> {
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

// --- Agent runner ---

/** Run an agent loop to completion and return the final assistant text. */
async function runAgent(sb: SpadeBox, initialMessages: Message[]): Promise<string> {
  const tools = sb.tools().map((t) => ({
    type: 'function' as const,
    function: {
      name: t.name,
      description: t.description,
      parameters: JSON.parse(t.inputSchema),
    },
  }))

  const messages = [...initialMessages]

  while (true) {
    const response = await chat(messages, tools)
    messages.push({ role: 'assistant', content: response.content, tool_calls: response.tool_calls })

    if (!response.tool_calls?.length) {
      return response.content ?? ''
    }

    for (const call of response.tool_calls) {
      const { name, arguments: args } = call.function
      console.log(`  ${BLUE}[call]${RESET} ${GRAY}${name}(${args})${RESET}`)
      const result = await sb.callTool(name, args)
      const tag = result.isError ? `  ${RED}[error]${RESET}` : `  ${GREEN}[ok]${RESET}`
      console.log(`${tag} ${GRAY}${result.output}${RESET}`)
      messages.push({ role: 'tool', tool_call_id: call.id, content: result.output })
    }
  }
}

// --- Setup main SpadeBox ---

const sb = new SpadeBox()
sb.enableFiles(sandboxPath).enableJs().enableHttp().allow('*', ['GET', 'HEAD'])

// Expose `subagent(prompt)` as a synchronous global in the JS REPL.
sb.exposeJsFunc('subagent', ['prompt'], async ({ prompt }) => {
  const task = String(prompt)
  console.log(`\n${YELLOW}[subagent]${RESET} ${GRAY}${task}${RESET}`)

  const subSb = new SpadeBox()
  subSb.enableFiles(sandboxPath).enableHttp().allow('*', ['GET', 'HEAD'])

  const result = await runAgent(subSb, [{ role: 'user', content: task }])
  console.log(`${YELLOW}[subagent done]${RESET} ${GRAY}${result}${RESET}`)
  return result
})

const mainTools = sb.tools().map((t) => ({
  type: 'function' as const,
  function: {
    name: t.name,
    description: t.description,
    parameters: JSON.parse(t.inputSchema),
  },
}))

// --- Interactive main agent loop ---

const SYSTEM_PROMPT =
  `You are a helpful orchestrator agent. You can delegate tasks to a subagent by calling the \`subagent(prompt)\` function inside the js_repl tool. The subagent has file and network access and runs to completion before returning its response.`

const messages: Message[] = [{ role: 'system', content: SYSTEM_PROMPT }]

async function runTurn(msgs: Message[]): Promise<void> {
  while (true) {
    const response = await chat(msgs, mainTools)
    msgs.push({ role: 'assistant', content: response.content, tool_calls: response.tool_calls })

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

      msgs.push({ role: 'tool', tool_call_id: call.id, content: result.output })
    }
  }
}

// --- Main ---

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
  try {
    rl.prompt()
  } catch { /* stdin closed during the last turn */ }
}
