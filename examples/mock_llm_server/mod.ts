/**
 * Stateless mock LLM server implementing the OpenAI chat completions API.
 *
 * Scenarios are defined in scenarios/mod.ts. The server selects the active
 * scenario by matching the first user message (stripped of whitespace) against
 * scenario IDs. Turn state is reconstructed from the full message history on
 * every request: the current turn index equals the number of assistant messages
 * already in the conversation. No session state is kept between requests.
 *
 * If the first user message does not match any scenario ID, the server replies
 * with an explanation and the list of available scenario IDs.
 *
 * Usage:
 *   deno run --allow-net mod.ts [--port <port>]
 *
 * Endpoints:
 *   POST /v1/chat/completions  — OpenAI-compatible chat completions
 *   POST /shutdown             — graceful shutdown
 */

import type { ChatCompletionRequest, Message, ScenarioTurn, ToolCall } from './types.ts'
import { scenarios } from './scenarios/mod.ts'

// =============================================================================
// Stateless turn reconstruction
// =============================================================================

/**
 * Scans all user messages in order for the first one that matches a scenario ID.
 * Walks the subsequent history to count completed turns and build a variable map
 * from any `capture` fields: the first tool result following each assistant
 * message is stored under the turn's capture name.
 */
function resolveScenario(
  messages: Message[],
): { scenarioId: string; turnIndex: number; vars: Record<string, string> } | null {
  let startIdx = -1
  let scenarioId = ''

  for (let i = 0; i < messages.length; i++) {
    if (messages[i].role !== 'user') continue
    const id = messages[i].content?.trim() ?? ''
    if (!scenarios.has(id)) continue
    startIdx = i
    scenarioId = id
    break
  }

  if (startIdx === -1) return null

  const scenario = scenarios.get(scenarioId)!
  const vars: Record<string, string> = {}
  const tail = messages.slice(startIdx + 1)
  let turnIndex = 0

  for (let i = 0; i < tail.length; i++) {
    if (tail[i].role !== 'assistant') continue
    const turn = scenario.turns[turnIndex]
    if (turn?.capture) {
      const toolMsg = tail.slice(i + 1).find((m) => m.role === 'tool')
      if (toolMsg?.content) vars[turn.capture] = toolMsg.content
    }
    turnIndex++
  }

  return { scenarioId, turnIndex, vars }
}

// =============================================================================
// Response builders
// =============================================================================

function interpolate(text: string, vars: Record<string, string>): string {
  return text.replace(/\{\{(\w+)\}\}/g, (_, name) => vars[name] ?? `{{${name}}}`)
}

function uid(prefix: string): string {
  return `${prefix}_${crypto.randomUUID().replace(/-/g, '').slice(0, 12)}`
}

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  })
}

function buildChatResponse(turn: ScenarioTurn, model: string, vars: Record<string, string>): Response {
  const { response } = turn
  const hasToolCalls = !!response.tool_calls?.length

  const toolCalls: ToolCall[] | undefined = hasToolCalls
    ? response.tool_calls!.map((tc) => ({
      id: uid('call'),
      type: 'function' as const,
      function: { name: tc.name, arguments: JSON.stringify(tc.arguments) },
    }))
    : undefined

  return json({
    id: uid('chatcmpl'),
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [
      {
        index: 0,
        message: {
          role: 'assistant',
          content: response.content ? interpolate(response.content, vars) : null,
          ...(toolCalls ? { tool_calls: toolCalls } : {}),
        },
        finish_reason: hasToolCalls ? 'tool_calls' : 'stop',
      },
    ],
    usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
  })
}

function buildNoScenarioResponse(model: string, scenarioIds: string[]): Response {
  const list = scenarioIds.map((id) => `  - ${id}`).join('\n')
  const content =
    `No active scenario. Send a message whose content is exactly a scenario ID to start one.\n\nAvailable scenarios:\n${list}`
  return json({
    id: uid('chatcmpl'),
    object: 'chat.completion',
    created: Math.floor(Date.now() / 1000),
    model,
    choices: [{ index: 0, message: { role: 'assistant', content }, finish_reason: 'stop' }],
    usage: { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 },
  })
}

// =============================================================================
// Endpoint handlers
// =============================================================================

async function chatCompletions(req: Request): Promise<Response> {
  const body = (await req.json()) as ChatCompletionRequest

  if (body.stream) {
    return json({ error: { message: 'streaming is not supported by the mock server' } }, 400)
  }

  const resolved = resolveScenario(body.messages)
  if (!resolved) {
    return buildNoScenarioResponse(body.model, [...scenarios.keys()])
  }

  const { scenarioId, turnIndex, vars } = resolved
  const scenario = scenarios.get(scenarioId)!

  if (turnIndex >= scenario.turns.length) {
    return json(
      { error: { message: `Scenario "${scenarioId}" is already complete (${scenario.turns.length} turn(s))` } },
      400,
    )
  }

  return buildChatResponse(scenario.turns[turnIndex], body.model, vars)
}

let shutdownResolve!: () => void
const shutdownPromise = new Promise<void>((res) => (shutdownResolve = res))

function shutdown(): Response {
  shutdownResolve()
  return json({ ok: true })
}

// =============================================================================
// HTTP handler
// =============================================================================

function handler(req: Request): Promise<Response> | Response {
  const { method, url } = req
  const path = new URL(url).pathname

  if (method === 'POST' && path === '/v1/chat/completions') return chatCompletions(req)
  if (method === 'POST' && path === '/shutdown') return shutdown()

  return new Response('Not Found', { status: 404 })
}

// =============================================================================
// Main
// =============================================================================

const args = Deno.args
const portFlag = args.indexOf('--port')
const port = portFlag !== -1 ? parseInt(args[portFlag + 1]) : 8324

const server = Deno.serve(
  { port, onListen: ({ port: p }) => console.log(`READY:${p}`) },
  handler,
)

await shutdownPromise
server.shutdown()
await server.finished
