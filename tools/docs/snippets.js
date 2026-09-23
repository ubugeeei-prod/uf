// @flow
//
// Every JavaScript sample in the manual parses.
//
//   UF_BIN=./target/release/uf node --import @uniflowed/host/register tools/docs/snippets.js
//
// The manual is several hundred code samples, and a sample is the part of a
// page a reader copies. One that does not parse is worse than none: it is
// copied, it fails, and the reader cannot tell whether the mistake is theirs.
// `uf check` refusing five samples' variance sigils (#1026) is how this
// repository found out that nothing was reading them.
//
// So each ```js, ```jsx and ```flow fence in `docs/app/**/*.mdx` is written to
// a file of its own and handed to `uf lint`, whose parser is the official Flow
// parser uf compiles with, and any `flow/syntax` diagnostic fails the check
// with the page and the line it is on. The sample is parsed as a module — a
// top-level `await` is fine, as it is in the module a reader pastes it into.
//
// Parsing is the whole of it. A sample is usually part of a module rather than
// a module — it names things declared elsewhere on the page — so resolving
// names or inferring types would fail most of the manual for reasons that are
// not mistakes.
//
// A fence that is deliberately not a whole program says so in its info string:
//
//   ```js fragment
//   images: { remotePatterns: [/* … */] },
//   ```
//
// — an object's properties out of their object, adjacent JSX elements, a
// signature with its body elided. `fragment` is the only opt-out, and the
// check prints how many fences use it, so a page cannot go quiet by marking
// everything.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const PAGES = path.join(REPO, "docs/app");

/** The languages checked. `ts` and `tsx` samples exist to be contrasted with Flow. */
const CHECKED: $ReadOnlyArray<string> = ["js", "jsx", "flow", "javascript"];

export type Fence = {|
  /** The page, relative to the repository. */
  readonly page: string,
  /** The line of the page the fence's first line of code is on, 1-based. */
  readonly line: number,
  readonly lang: string,
  /** Everything after the language in the info string. */
  readonly meta: string,
  readonly code: string,
|};

/** Every fenced code block in a Markdown page, in order. */
export function fences(page: string, markdown: string): Array<Fence> {
  const lines = markdown.split("\n");
  const out: Array<Fence> = [];
  let open: {| marker: string, lang: string, meta: string, start: number |} | null = null;
  let body: Array<string> = [];
  lines.forEach((line, index) => {
    if (open == null) {
      const start = line.match(/^(\s*)(`{3,}|~{3,})\s*([\w-]*)\s*(.*)$/);
      if (start != null) {
        open = { marker: start[2], lang: start[3], meta: start[4].trim(), start: index + 2 };
        body = [];
      }
      return;
    }
    const current = open;
    if (line.trim().startsWith(current.marker) && line.trim().replace(/[`~]/g, "") === "") {
      out.push({
        page,
        line: current.start,
        lang: current.lang,
        meta: current.meta,
        code: body.join("\n"),
      });
      open = null;
      return;
    }
    body.push(line);
  });
  return out;
}

/** Whether a fence is a sample this check holds to parsing. */
export function isChecked(fence: Fence): boolean {
  return CHECKED.includes(fence.lang) && !/(^|\s)fragment(\s|$)/.test(fence.meta);
}

/**
 * The file a fence is checked as: Flow, and a module, with the sample's own
 * lines where they were so a diagnostic's line maps back by one subtraction.
 */
export function asModule(fence: Fence): string {
  return `// @flow\n${fence.code}\nexport {};\n`;
}

/** A sample that did not parse, where a person would go to fix it. */
export type Failure = {| readonly page: string, readonly line: number, readonly message: string |};

/**
 * `uf lint --json`'s syntax diagnostics, mapped from the scratch files back to
 * the pages. `files` maps a scratch file's name to the fence it holds.
 */
export function failures(report: mixed, files: $ReadOnlyMap<string, Fence>): Array<Failure> {
  const diagnostics =
    report != null && typeof report === "object" && Array.isArray(report.diagnostics)
      ? report.diagnostics
      : [];
  const out: Array<Failure> = [];
  // One failure per sample: the parser's first complaint is the mistake, and
  // the rest are it recovering from that one.
  const seen = new Set<string>();
  for (const diagnostic of diagnostics) {
    if (diagnostic == null || typeof diagnostic !== "object" || diagnostic.rule !== "flow/syntax") {
      continue;
    }
    const file = typeof diagnostic.path === "string" ? path.basename(diagnostic.path) : "";
    const fence = files.get(file);
    const line = typeof diagnostic.line === "number" ? diagnostic.line : 1;
    const message = typeof diagnostic.message === "string" ? diagnostic.message : "does not parse";
    if (fence != null && !seen.has(file)) {
      seen.add(file);
      // Line 1 of the scratch file is the pragma, so line 2 is the fence's first.
      out.push({ page: fence.page, line: fence.line + Math.max(0, line - 2), message });
    }
  }
  return out.sort((a, b) => a.page.localeCompare(b.page) || a.line - b.line);
}

function pagesUnder(dir: string): Array<string> {
  const found = [];
  for (const name of fs.readdirSync(dir)) {
    const full = path.join(dir, name);
    if (fs.statSync(full).isDirectory()) {
      found.push(...pagesUnder(full));
    } else if (name.endsWith(".mdx")) {
      found.push(full);
    }
  }
  return found.sort();
}

function main(): void {
  const uf = process.env.UF_BIN ?? "uf";
  const all: Array<Fence> = [];
  for (const file of pagesUnder(PAGES)) {
    all.push(...fences(path.relative(REPO, file), fs.readFileSync(file, "utf8")));
  }
  const checked = all.filter(isChecked);
  const fragments = all.filter((fence) => CHECKED.includes(fence.lang) && !isChecked(fence));

  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "uf-docs-snippets-"));
  try {
    fs.writeFileSync(
      path.join(scratch, "package.json"),
      '{ "name": "snippets", "private": true }\n',
    );
    const files = new Map<string, Fence>();
    checked.forEach((fence, index) => {
      const name = `sample-${index}.js`;
      files.set(name, fence);
      fs.writeFileSync(path.join(scratch, name), asModule(fence));
    });
    let out = "";
    try {
      out = execFileSync(uf, ["lint", "--json", "."], {
        cwd: scratch,
        encoding: "utf8",
        env: { ...process.env, NO_COLOR: "1" },
        stdio: ["ignore", "pipe", "ignore"],
        maxBuffer: 64 * 1024 * 1024,
      });
    } catch (error) {
      // `uf lint` exits non-zero when it found anything, which on samples
      // written without their imports' packages is always. The report is on
      // stdout either way.
      out = typeof error?.stdout === "string" ? error.stdout : "";
    }
    if (out.trim() === "") {
      throw new Error("uf lint --json printed nothing");
    }
    const broken = failures(JSON.parse(out), files);
    process.stdout.write(
      `docs-snippets: ${checked.length} samples parsed, ${fragments.length} marked as fragments\n`,
    );
    if (broken.length > 0) {
      process.stdout.write(
        `\nThese samples do not parse. Fix each one, or mark a sample that is deliberately part of a program \`\`\`js fragment:\n${broken
          .map((failure) => `  ${failure.page}:${failure.line}: ${failure.message}`)
          .join("\n")}\n`,
      );
      process.exitCode = 1;
    }
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
}

if (process.argv[1] != null && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
