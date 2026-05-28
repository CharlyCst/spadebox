// Output:
// hello from module

import { readFileSync, writeFileSync } from 'node:fs'

writeFileSync('fs_import_tmp.txt', 'hello from module')
console.log(readFileSync('fs_import_tmp.txt'))
