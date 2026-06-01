import type { Scenario } from '../types.ts'

// The main agent calls js_repl with subagent('read_file').
// The subagent function spawns a fresh agent that runs through the 'read_file'
// scenario and returns "The file says: hello from sandbox." as its final response.
// js_repl returns that string quoted (Boa's display format for strings).
export default {
  turns: [
    {
      capture: 'result',
      response: {
        tool_calls: [{ name: 'js_repl', arguments: { code: "subagent('read_file')" } }],
      },
    },
    {
      response: {
        content: 'The subagent completed the task and reported: {{result}}',
      },
    },
  ],
} satisfies Scenario
