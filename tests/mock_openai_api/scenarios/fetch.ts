import type { Scenario } from '../types.ts'

export default {
  turns: [
    {
      capture: 'page_content',
      response: {
        tool_calls: [{ name: 'fetch', arguments: { url: 'https://example.com', method: 'GET' } }],
      },
    },
    {
      response: {
        content: 'Here is the content of example.com:\n\n{{page_content}}',
      },
    },
  ],
} satisfies Scenario
