A uf project declares its tasks in `uf.config.js`, where `uf run` caches them, orders their dependencies and runs them the same way in CI. Shelling out to `npm run`, `yarn`, `pnpm` or `npx` from source goes around all of that and ties the code to one package manager. Declare a task and run it with `uf run`.

## Bad

```js
// @flow
import { execSync } from "node:child_process";

execSync("npm run build");
execSync("pnpm install");
```

```diagnostics
app/example.js:4:11 declare the task in uf.config.js; uf projects do not shell out to npm/yarn/pnpm/bunx
app/example.js:5:11 declare the task in uf.config.js; uf projects do not shell out to npm/yarn/pnpm/bunx
```

## Good

```js
// @flow
import { execSync } from "node:child_process";

execSync("uf run build");

// A manager named as data, not run: a union a config accepts.
export type Manager = "npm" | "pnpm" | "yarn";
```
