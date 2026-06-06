// Output:
// writeFileSync / readFileSync: hello world
// appendFileSync: line1,line2
// existsSync: true false
// readdirSync: a.txt,b.txt
// mkdirSync: dir exists, nested exists
// statSync file: size=5 isFile=true isDir=false mtimeMs>0=true
// statSync dir: isFile=false isDir=true
// unlinkSync: removed
// renameSync: old gone, new=moved
// copyFileSync: src=data, dst=data
// rmSync recursive: removed
// rmSync force: ok

// writeFileSync / readFileSync
fs.writeFileSync('hello.txt', 'hello world')
console.log('writeFileSync / readFileSync:', fs.readFileSync('hello.txt'))

// appendFileSync
fs.writeFileSync('log.txt', 'line1')
fs.appendFileSync('log.txt', ',line2')
console.log('appendFileSync:', fs.readFileSync('log.txt'))

// existsSync
console.log('existsSync:', fs.existsSync('hello.txt'), fs.existsSync('nope.txt'))

// readdirSync
fs.writeFileSync('a.txt', '')
fs.writeFileSync('b.txt', '')
const entries = fs.readdirSync('.').filter((f) => f === 'a.txt' || f === 'b.txt').sort()
console.log('readdirSync:', entries.join(','))

// mkdirSync
fs.mkdirSync('mydir')
fs.mkdirSync('nested/a/b', { recursive: true })
console.log(
  `mkdirSync: ${fs.existsSync('mydir') ? 'dir exists' : 'missing'}, ${
    fs.existsSync('nested/a/b') ? 'nested exists' : 'missing'
  }`,
)

// statSync
fs.writeFileSync('stat.txt', 'hello')
const fstat = fs.statSync('stat.txt')
console.log(
  'statSync file:',
  `size=${fstat.size}`,
  `isFile=${fstat.isFile()}`,
  `isDir=${fstat.isDirectory()}`,
  `mtimeMs>0=${fstat.mtimeMs > 0}`,
)
const dstat = fs.statSync('mydir')
console.log('statSync dir:', `isFile=${dstat.isFile()}`, `isDir=${dstat.isDirectory()}`)

// unlinkSync
fs.writeFileSync('del.txt', 'gone')
fs.unlinkSync('del.txt')
console.log('unlinkSync:', fs.existsSync('del.txt') ? 'still there' : 'removed')

// renameSync
fs.writeFileSync('old.txt', 'moved')
fs.renameSync('old.txt', 'new.txt')
console.log('renameSync:', `old gone,`, `new=${fs.readFileSync('new.txt')}`)

// copyFileSync
fs.writeFileSync('src.txt', 'data')
fs.copyFileSync('src.txt', 'dst.txt')
console.log('copyFileSync:', `src=${fs.readFileSync('src.txt')},`, `dst=${fs.readFileSync('dst.txt')}`)

// rmSync recursive
fs.mkdirSync('tree/sub', { recursive: true })
fs.writeFileSync('tree/sub/f.txt', 'x')
fs.rmSync('tree', { recursive: true })
console.log('rmSync recursive:', fs.existsSync('tree') ? 'still there' : 'removed')

// rmSync force
fs.rmSync('does_not_exist.txt', { force: true })
console.log('rmSync force: ok')
