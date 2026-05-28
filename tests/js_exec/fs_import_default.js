// Output:
// hello again

import fs from 'fs'

fs.writeFileSync('fs_import_default_tmp.txt', 'hello again')
console.log(fs.readFileSync('fs_import_default_tmp.txt'))
