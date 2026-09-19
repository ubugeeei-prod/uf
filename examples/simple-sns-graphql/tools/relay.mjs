import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
const require = createRequire(import.meta.url);
const compiler = require.resolve("relay-compiler/cli.js");
const result = spawnSync(process.execPath, [compiler, ...process.argv.slice(2)], {
  cwd: new URL("..", import.meta.url),
  stdio: "inherit",
});
if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
