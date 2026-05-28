// Output:
// 42

async function compute(x) {
  return x * 2
}

compute(21).then(r => console.log(r))
