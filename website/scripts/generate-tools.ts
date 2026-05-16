import { dirname, join } from '@std/path'

const SCRIPT_DIR = dirname(import.meta.filename!)
const WORKSPACE_ROOT = join(SCRIPT_DIR, '..', '..')
const DOCS_TOOLS_DIR = join(SCRIPT_DIR, '..', 'docs', 'tools')

async function cargo(...args: string[]): Promise<string> {
  const cmd = new Deno.Command('cargo', {
    args: ['run', '--quiet', '-p', 'spadebox-cli', '--', ...args],
    cwd: WORKSPACE_ROOT,
    stdout: 'piped',
    stderr: 'inherit',
  })
  const { code, stdout } = await cmd.output()
  if (code !== 0) throw new Error(`cargo run failed: ${args.join(' ')}`)
  return new TextDecoder().decode(stdout).trim()
}

function parseToolNames(listOutput: string): string[] {
  return listOutput
    .split('\n')
    .filter((line) => line.length > 0 && !line.startsWith(' '))
    .map((line) => line.trim())
}

function stripTitle(markdown: string): string {
  const lines = markdown.split('\n')
  // Drop the "## Tool: name" heading and the blank line that follows it.
  if (lines[0].startsWith('## Tool:')) {
    const start = lines[1] === '' ? 2 : 1
    return lines.slice(start).join('\n')
  }
  return markdown
}

const tools = parseToolNames(await cargo('tools', 'list'))
console.log(`Generating docs for: ${tools.join(', ')}`)

await Promise.all(
  tools.map(async (tool, index) => {
    const markdown = await cargo('tools', 'info', tool, '--markdown')
    const frontmatter = `---\nsidebar_position: ${index + 1}\n---\n\n`
    const outPath = join(DOCS_TOOLS_DIR, `${tool}.md`)
    await Deno.writeTextFile(outPath, frontmatter + stripTitle(markdown) + '\n')
    console.log(`  wrote ${outPath}`)
  }),
)

console.log('Done.')
