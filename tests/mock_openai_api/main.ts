/**
 * CLI entry point for the mock OpenAI API server.
 *
 * Usage:
 *   deno run --allow-net main.ts [--port <port>]
 *
 * Endpoints:
 *   POST /v1/chat/completions  — OpenAI-compatible chat completions
 *   POST /shutdown             — graceful shutdown
 */

import { start } from './mod.ts'

const args = Deno.args
const portFlag = args.indexOf('--port')
const port = portFlag !== -1 ? parseInt(args[portFlag + 1]) : 8324

const server = await start({ port })
console.log(`READY:${server.port}`)
await server.finished
