// @noflow
//
// The process `uf lint` runs a project's own rules in. The driver — when it is
// started, what it may spend, and why it is a process at all — is
// `crates/uf_cli/src/commands/lint/plugins.rs`; what a rule sees is
// `./internal/lint-rules.js`.
//
// One JSON object per line, each way:
//
//   → {"type": "load", "root": "/project", "modules": ["/project/rules/acme.js"], "rules": ["acme/no-foo"]}
//   ← {"type": "loaded", "problems": []}
//   → {"type": "lint", "path": "app.js", "filename": "/project/app.js", "source": "…", "ast": {…}}
//   ← {"type": "linted", "diagnostics": [{"rule": "acme/no-foo", "message": "…", "start": 13, "end": 16, "fix": null}], "micros": {"acme/no-foo": 41.2}, "problems": []}
//
// Stdout is the protocol, so nothing else may write to it: a rule's
// `console.log` would land in the middle of a reply and be read as a broken
// one. Everything a rule prints goes to stderr instead, which `uf` passes
// through to the terminal.

import { createInterface } from "node:readline";

import { lintFile, loadRules } from "./internal/lint-rules.js";

const reply = process.stdout.write.bind(process.stdout);
process.stdout.write = process.stderr.write.bind(process.stderr);
for (const method of ["debug", "dir", "info", "log", "table", "trace"]) {
  console[method] = console.error;
}

let loaded = { root: process.cwd(), rules: [], problems: [] };
const lines = createInterface({ input: process.stdin, crlfDelay: Infinity });
for await (const line of lines) {
  if (line.trim() === "") {
    continue;
  }
  const request = JSON.parse(line);
  if (request.type === "load") {
    loaded = await loadRules(request.root, request.modules, request.rules);
    send({ type: "loaded", problems: loaded.problems });
  } else if (request.type === "lint") {
    send({ type: "linted", ...lintFile(loaded, request) });
  }
}

function send(message) {
  reply(`${JSON.stringify(message)}\n`);
}
