A module that uses both `import` and `require` runs through two module systems, with different loading order and interop rules. Keep an ES module on `import`. When a CommonJS-only file has to be loaded, use the `createRequire` bridge that Node documents.

## Bad

```js
// @flow
import { readFile } from "node:fs/promises";
const path = require("node:path");

export async function read(name: string): Promise<string> {
  return readFile(path.join("data", name), "utf8");
}
```

```diagnostics
app/example.js:3:14 this module already uses `import`; do not mix in `require`
```

## Good

```js
// @flow
import { readFile } from "node:fs/promises";
import path from "node:path";

export async function read(name: string): Promise<string> {
  return readFile(path.join("data", name), "utf8");
}
```
