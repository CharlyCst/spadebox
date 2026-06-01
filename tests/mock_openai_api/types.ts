// =============================================================================
// OpenAI API types
// =============================================================================

export interface Message {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string | null
  tool_call_id?: string
  tool_calls?: ToolCall[]
}

export interface ToolCall {
  id: string
  type: 'function'
  function: { name: string; arguments: string }
}

export interface ChatCompletionRequest {
  model: string
  messages: Message[]
  tools?: unknown[]
  tool_choice?: unknown
  stream?: boolean
}

// =============================================================================
// Scenario types
// =============================================================================

export interface ScenarioTurn {
  /** Save the first tool result of this turn into a named variable for later interpolation. */
  capture?: string
  response: {
    /** Plain-text reply from the assistant. */
    content?: string
    /** Tool calls the assistant wants to make. */
    tool_calls?: Array<{
      name: string
      arguments: Record<string, unknown>
    }>
  }
}

export interface Scenario {
  turns: ScenarioTurn[]
}
