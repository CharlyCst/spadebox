// Output:
// 42

// deno-lint-ignore require-await
async function compute(x) {
  return x * 2
}

compute(21).then((r) => console.log(r))
