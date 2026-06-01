import type { Scenario } from '../types.ts'

export default {
  turns: [
    {
      capture: 'file_content',
      response: {
        tool_calls: [{ name: 'read_file', arguments: { path: 'hello.txt' } }],
      },
    },
    {
      response: {
        content: 'The file says: {{file_content}}.',
      },
    },
  ],
} satisfies Scenario
