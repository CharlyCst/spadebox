import type { Scenario } from '../types.ts'
import fetch from './fetch.ts'
import readFile from './read_file.ts'
import subagent from './subagent.ts'

export const scenarios = new Map<string, Scenario>([
  ['read_file', readFile],
  ['fetch', fetch],
  ['subagent', subagent],
])
