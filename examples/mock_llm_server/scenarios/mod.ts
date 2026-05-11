import type { Scenario } from '../types.ts'
import fetch from './fetch.ts'
import readFile from './read_file.ts'

export const scenarios = new Map<string, Scenario>([
  ['read_file', readFile],
  ['fetch', fetch],
])
