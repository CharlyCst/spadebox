import { runJsRuntime, type JsRuntimeResult } from './js_runtime.ts'
import { runExamples, type ExampleResult } from './examples.ts'

function reportJsRuntime(results: JsRuntimeResult[]): number {
  console.log(`\nJS runtime (${results.length} test(s))\n`)
  let failed = 0
  for (const r of results) {
    if (r.pass) {
      console.log(`  ✓ ${r.file}`)
    } else {
      console.log(`  ✗ ${r.file}${r.error ? ` (${r.error})` : ''}`)
      console.log(`    expected: ${JSON.stringify(r.expected)}`)
      console.log(`    actual:   ${JSON.stringify(r.actual)}`)
      failed++
    }
  }
  return failed
}

function reportExamples(results: ExampleResult[]): number {
  console.log(`\nExamples (${results.length} test(s))\n`)
  let failed = 0
  for (const r of results) {
    if (r.pass) {
      console.log(`  ✓ ${r.name}/${r.scenario}`)
    } else {
      console.log(`  ✗ ${r.name}/${r.scenario}${r.error ? ` (${r.error})` : ''}`)
      if (r.output) console.log(`    output: ${r.output.slice(0, 200)}`)
      failed++
    }
  }
  return failed
}

const jsResults = await runJsRuntime()
const exampleResults = await runExamples()

const failedJs = reportJsRuntime(jsResults)
const failedExamples = reportExamples(exampleResults)

const totalPassed = jsResults.length - failedJs + (exampleResults.length - failedExamples)
const totalFailed = failedJs + failedExamples
console.log(`\n${totalPassed} passed, ${totalFailed} failed`)

if (totalFailed > 0) Deno.exit(1)
