/**
 * Inject code snippets from source files into MDX documentation.
 *
 * Source files are tagged with region markers:
 *   // [snippet: region-name]
 *   ... code ...
 *   // [/snippet]
 *
 * Doc files reference a region with:
 *   {/* snippet: path/to/file.ts#region-name *\/}
 *   ```ts
 *   ... replaced on each run ...
 *   ```
 *   {/* /snippet *\/}
 */
import { dirname, extname, join } from '@std/path'

const SCRIPT_DIR = dirname(import.meta.filename!)
const DOCS_DIR = join(SCRIPT_DIR, '..', 'docs')
const WORKSPACE_ROOT = join(SCRIPT_DIR, '..', '..')

const EXT_TO_LANG: Record<string, string> = {
  '.rs': 'rust',
  '.ts': 'ts',
  '.js': 'js',
  '.py': 'python',
  '.toml': 'toml',
  '.sh': 'bash',
}

function langFor(filePath: string): string {
  return EXT_TO_LANG[extname(filePath)] ?? ''
}

function extractSnippet(source: string, region: string, sourcePath: string): string {
  const lines = source.split('\n')
  const startTag = `[snippet: ${region}]`
  const endTag = `[/snippet]`

  const start = lines.findIndex((l) => l.includes(startTag))
  if (start === -1) throw new Error(`Snippet "${region}" not found in ${sourcePath}`)

  const end = lines.findIndex((l, i) => i > start && l.includes(endTag))
  if (end === -1) throw new Error(`End of snippet "${region}" not found in ${sourcePath}`)

  const snippet = lines.slice(start + 1, end)
  const indent = Math.min(
    ...snippet.filter((l) => l.trim().length > 0).map((l) => l.match(/^(\s*)/)![1].length),
  )
  return snippet.map((l) => l.slice(indent)).join('\n').trimEnd()
}

// Matches {/* snippet: path#region */} ... {/* /snippet */}
const SNIPPET_RE = /\{\/\* snippet: (.+?)#(.+?) \*\/\}([\s\S]*?)\{\/\* \/snippet \*\/\}/g

async function processFile(docPath: string): Promise<number> {
  const content = await Deno.readTextFile(docPath)
  let count = 0

  const updated = content.replace(SNIPPET_RE, (_, filePath: string, region: string) => {
    const absPath = join(WORKSPACE_ROOT, filePath)
    const source = Deno.readTextFileSync(absPath)
    const code = extractSnippet(source, region, filePath)
    const lang = langFor(filePath)
    count++
    return `{/* snippet: ${filePath}#${region} */}\n\`\`\`${lang}\n${code}\n\`\`\`\n{/* /snippet */}`
  })

  if (count > 0) await Deno.writeTextFile(docPath, updated)
  return count
}

async function* walkDocs(dir: string): AsyncGenerator<string> {
  for await (const entry of Deno.readDir(dir)) {
    const path = join(dir, entry.name)
    if (entry.isDirectory) yield* walkDocs(path)
    else if (entry.name.endsWith('.md') || entry.name.endsWith('.mdx')) yield path
  }
}

let total = 0
for await (const path of walkDocs(DOCS_DIR)) {
  const count = await processFile(path)
  if (count > 0) {
    console.log(`  ${path}: ${count} snippet(s) injected`)
    total += count
  }
}

console.log(`Done — ${total} snippet(s) total.`)
