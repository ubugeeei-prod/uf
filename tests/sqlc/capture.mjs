// Capture the `plugin.GenerateRequest` sqlc sends for every case in
// `sqlc.json`, as `cases/<case>/request.json`.
//
//   node tests/sqlc/capture.mjs            # rewrite the fixtures
//   node tests/sqlc/capture.mjs --check    # fail if sqlc would send something else
//
// `crates/uf_sqlc/tests/golden.rs` generates from these fixtures, so the Rust
// suite runs without sqlc installed; `--check` is how CI notices when a sqlc
// upgrade changes what a plugin receives. `SQLC` names the binary (default:
// `sqlc` on PATH).
//
// Two things are removed on the way to disk, both so that a reviewer can read
// the diff: PostgreSQL's own `pg_catalog` and `information_schema` schemas
// (a megabyte of catalog the generator skips), and every field at its proto3
// default, which the JSON form spells out and a decoder fills back in.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { isDeepStrictEqual } from "node:util";

const here = path.dirname(new URL(import.meta.url).pathname);
const check = process.argv.includes("--check");
const sqlc = process.env.SQLC ?? "sqlc";

// Cases also kept as protobuf, the encoding sqlc sends by default, so the
// Rust suite can check its wire decoder against the JSON it was written from.
const PROTOBUF_CASES = new Set(["types-sqlite", "types-mysql"]);

const work = fs.mkdtempSync(path.join(os.tmpdir(), "uf-sqlc-capture-"));
const config = JSON.parse(fs.readFileSync(path.join(here, "sqlc.json"), "utf8"));

// The plugin is a relative path beside the config, so that the command sqlc
// records in the request is the same on every machine. sqlc resolves paths
// against the config file's directory, so the rewritten config lives there too.
const plugin = path.join(here, ".capture.sh");
const temporary = path.join(here, ".sqlc.capture.json");

function capture(entries, format, extension) {
  fs.writeFileSync(
    plugin,
    `#!/bin/sh\ncat > "$UF_SQLC_CAPTURE/$$.${extension}"\n${format === "json" ? 'printf "{}"\n' : ""}`,
    { mode: 0o755 },
  );
  const rewritten = {
    ...config,
    plugins: [
      { name: "flow", env: ["UF_SQLC_CAPTURE"], process: { cmd: "./.capture.sh", format } },
    ],
    sql: entries,
  };
  fs.writeFileSync(temporary, JSON.stringify(rewritten));
  try {
    execFileSync(sqlc, ["generate", "-f", temporary], {
      cwd: here,
      env: { ...process.env, UF_SQLC_CAPTURE: work },
      stdio: ["ignore", "inherit", "inherit"],
    });
  } finally {
    fs.rmSync(temporary, { force: true });
    fs.rmSync(plugin, { force: true });
  }
}

const caseName = (entry) => path.basename(path.dirname(entry.codegen[0].out));
capture(config.sql, "json", "json");
let stale = 0;
for (const entry of config.sql.filter((entry) => PROTOBUF_CASES.has(caseName(entry)))) {
  const before = new Set(fs.readdirSync(work));
  capture([entry], "", "pb");
  const [file] = fs.readdirSync(work).filter((name) => !before.has(name));
  const bytes = fs.readFileSync(path.join(work, file));
  const target = path.join(here, "cases", caseName(entry), "request.pb");
  if (fs.existsSync(target) && Buffer.compare(fs.readFileSync(target), bytes) === 0) {
    continue;
  }
  if (check) {
    console.error(`stale: ${path.relative(process.cwd(), target)}`);
    stale += 1;
  } else {
    fs.writeFileSync(target, bytes);
    console.log(`wrote ${path.relative(process.cwd(), target)}`);
  }
}

const SYSTEM = new Set(["pg_catalog", "information_schema"]);

function compact(value) {
  if (Array.isArray(value)) {
    // Elements stay, even at their default: `["", "a"]` is two enum labels.
    const items = value.map((item) =>
      item !== null && typeof item === "object" ? (compact(item) ?? {}) : item,
    );
    return items.length === 0 ? undefined : items;
  }
  if (value !== null && typeof value === "object") {
    const out = {};
    for (const [key, item] of Object.entries(value)) {
      const kept = compact(item);
      if (kept !== undefined) {
        out[key] = kept;
      }
    }
    return Object.keys(out).length === 0 ? undefined : out;
  }
  if (value === null || value === false || value === 0 || value === "") {
    return undefined;
  }
  return value;
}

const seen = new Set();
let wrote = 0;
for (const file of fs.readdirSync(work).filter((name) => name.endsWith(".json"))) {
  const request = JSON.parse(fs.readFileSync(path.join(work, file), "utf8"));
  const out = request.settings.codegen.out;
  const name = path.basename(path.dirname(out));
  seen.add(name);
  request.catalog.schemas = request.catalog.schemas.filter((schema) => !SYSTEM.has(schema.name));
  // The plugin's own invocation, which differs between this script and a real run.
  request.settings.codegen.process = { cmd: "uf" };
  delete request.settings.codegen.env;
  const compacted = compact(request);
  const target = path.join(here, "cases", name, "request.json");
  // Compared as values, not as text: the fixtures are checked in formatted
  // (`uf fmt --check` runs over this project), and the formatter lays JSON
  // out differently from `JSON.stringify`.
  const before = fs.existsSync(target) ? JSON.parse(fs.readFileSync(target, "utf8")) : undefined;
  if (isDeepStrictEqual(before, compacted)) {
    continue;
  }
  if (check) {
    console.error(`stale: ${path.relative(process.cwd(), target)}`);
    stale += 1;
  } else {
    fs.writeFileSync(target, `${JSON.stringify(compacted, null, 2)}\n`);
    console.log(`wrote ${path.relative(process.cwd(), target)}`);
    wrote += 1;
  }
}
fs.rmSync(work, { recursive: true, force: true });
for (const entry of config.sql) {
  const name = caseName(entry);
  if (!seen.has(name)) {
    console.error(`sqlc sent no request for ${name}`);
    stale += 1;
  }
}
if (wrote > 0) {
  console.log("now run `uf fmt` in tests/sqlc, which lays the rewritten JSON out as it is checked in");
}
if (stale > 0) {
  console.error(
    check ? "run `node tests/sqlc/capture.mjs` and commit the fixtures" : "capture incomplete",
  );
  process.exit(1);
}
