// @flow
//
// What uf ships that the manual does not document.
//
//   UF_BIN=./target/release/uf node --import @uniflowed/host/register tools/docs/coverage.js
//   ... tools/docs/coverage.js --update     rewrite the list of known gaps
//
// Three surfaces, each read from the thing that ships rather than from a list
// somebody keeps:
//
//   * commands and flags — `uf --help`, walked down every subcommand. A
//     command is documented when `docs/app/reference/cli` has a heading naming
//     it; a flag, when it appears under that heading.
//   * configuration keys — `uf inspect --json` in an empty directory, which is
//     every key the loader serializes with its default. (`crates/uf_config`'s
//     `flow_schema` test holds that set equal to what `@uniflowed/config`
//     declares, so it is also every key a project can write.) A key is
//     documented when `docs/app/reference/config` names it — in full, or
//     relative to the `##` section for its first segment, which is how the
//     reference's tables write them.
//   * packages — every `packages/*` that is not private. Documented when
//     `docs/app/reference/packages` names it.
//
// Every gap is compared with `tools/docs/coverage-gaps.txt`, the known list.
// A gap not on the list fails: something shipped without a page. A line on the
// list that is no longer a gap fails too: it was documented, and the list has
// to shrink with it, so the list is always the true remainder and never a
// place where a regression can hide behind an old entry.
//
// The matching is textual on purpose. It cannot say whether a page is *good*,
// only whether the thing is named where a reader looking it up would look.
// That is the part that can be checked without a person, and it is the part
// that silently drifts.

import { execFileSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const REPO = path.resolve(path.dirname(fileURLToPath(String(import.meta.url))), "..", "..");
const GAPS = path.join(REPO, "tools/docs/coverage-gaps.txt");
const CLI_PAGE = "docs/app/reference/cli/$page.mdx";
const CONFIG_PAGE = "docs/app/reference/config/$page.mdx";
const PACKAGES_PAGE = "docs/app/reference/packages/$page.mdx";

/** Flags every command takes, documented once under "Global flags". */
export const GLOBAL_FLAGS: $ReadOnlyArray<string> = ["--cwd", "--color", "--help", "--version"];

export type Help = {|
  readonly subcommands: $ReadOnlyArray<string>,
  readonly flags: $ReadOnlyArray<string>,
|};

/**
 * The subcommands and long flags one `--help` screen lists.
 *
 * clap's layout: a `Commands:` block of two-space-indented names, and option
 * blocks whose entries start with `-x, --long` or `--long`. `help` is clap's
 * own and is not a uf command.
 */
export function parseHelp(screen: string): Help {
  const subcommands: Array<string> = [];
  const flags: Array<string> = [];
  let block: "commands" | "other" = "other";
  for (const line of screen.split("\n")) {
    if (/^\S.*:\s*$/.test(line)) {
      block = /^(Commands|Subcommands):/.test(line) ? "commands" : "other";
      continue;
    }
    if (block === "commands") {
      const name = line.match(/^ {2}([a-z][\w-]*)(?:\s|$)/);
      if (name != null && name[1] !== "help") {
        subcommands.push(name[1]);
      }
      continue;
    }
    const flag = line.match(/^\s+(?:-\w, )?(--[a-z][\w-]*)/);
    if (flag != null && !flags.includes(flag[1])) {
      flags.push(flag[1]);
    }
  }
  return { subcommands, flags };
}

/** A heading of a Markdown page, and everything under it until the next of its rank or above. */
export type Section = {| readonly heading: string, readonly depth: number, readonly body: string |};

/** The `#`–`####` sections of a page. Fenced code is not searched for headings. */
export function sections(markdown: string): Array<Section> {
  const lines = markdown.split("\n");
  const heads: Array<{| line: number, depth: number, text: string |}> = [];
  let fenced = false;
  lines.forEach((line, index) => {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      return;
    }
    const match = fenced ? null : line.match(/^(#{1,4})\s+(.*)$/);
    if (match != null) {
      heads.push({ line: index, depth: match[1].length, text: match[2] });
    }
  });
  return heads.map((head, index) => {
    let end = lines.length;
    for (const later of heads.slice(index + 1)) {
      if (later.depth <= head.depth) {
        end = later.line;
        break;
      }
    }
    return {
      heading: head.text.replaceAll("`", ""),
      depth: head.depth,
      body: lines.slice(head.line, end).join("\n"),
    };
  });
}

/** Whether `phrase` occurs in `text` with no word character on either side. */
function standsAlone(text: string, phrase: string): boolean {
  let from = 0;
  for (;;) {
    const at = text.indexOf(phrase, from);
    if (at === -1) {
      return false;
    }
    const before = at === 0 ? "" : text[at - 1];
    const after = text[at + phrase.length] ?? "";
    if (!/[\w-]/.test(before) && !/[\w-]/.test(after)) {
      return true;
    }
    from = at + 1;
  }
  return false;
}

/** The sections of the command reference whose heading names `uf <command>`. */
export function commandSections(page: $ReadOnlyArray<Section>, command: string): Array<Section> {
  return page.filter((section) => standsAlone(section.heading, `uf ${command}`));
}

/** Every inline code span of a page, outside fenced blocks. */
export function codeSpans(markdown: string): Array<string> {
  const spans = [];
  let fenced = false;
  for (const line of markdown.split("\n")) {
    if (/^\s*(```|~~~)/.test(line)) {
      fenced = !fenced;
      continue;
    }
    if (fenced) {
      continue;
    }
    for (const match of line.matchAll(/`([^`]+)`/g)) {
      spans.push(match[1].trim());
    }
  }
  return spans;
}

/**
 * The key paths the configuration reference names.
 *
 * In full (`app.router.basePath`), or relative to the `##` section for their
 * first segment (`router.basePath` under `## app`), including a heading that
 * names several (`## pm and publish`). A table row that continues a key as
 * `.fetch` after `rendering.cache.route` names `rendering.cache.fetch`, and
 * `react.*` names everything under `react`.
 */
export function namedKeys(markdown: string): {|
  readonly exact: $ReadOnlySet<string>,
  readonly prefixes: $ReadOnlyArray<string>,
|} {
  const exact = new Set<string>();
  const prefixes: Array<string> = [];
  const add = (key: string) => {
    if (key.endsWith(".*")) {
      prefixes.push(key.slice(0, -1));
    } else {
      exact.add(key);
    }
  };
  for (const section of sections(markdown)) {
    if (section.depth !== 2) {
      continue;
    }
    const tops = section.heading.match(/[A-Za-z][\w]*/g) ?? [];
    let fenced = false;
    for (const line of section.body.split("\n")) {
      if (/^\s*(```|~~~)/.test(line)) {
        fenced = !fenced;
      }
      if (fenced) {
        continue;
      }
      let previous: string | null = null;
      for (const span of codeSpans(line)) {
        if (!/^\.?[A-Za-z][\w.]*(\.\*)?$/.test(span)) {
          continue;
        }
        const key =
          span.startsWith(".") && previous != null
            ? `${previous.slice(0, previous.lastIndexOf("."))}${span}`
            : span;
        previous = key;
        add(key);
        for (const top of tops) {
          add(`${top}.${key}`);
        }
      }
    }
  }
  for (const span of codeSpans(markdown)) {
    if (/^[A-Za-z][\w.]*(\.\*)?$/.test(span)) {
      add(span);
    }
  }
  return { exact, prefixes };
}

/**
 * Maps whose keys are the project's own rather than uf's: rule names, task
 * names, scopes. Each is one key to document, not one per entry — the same
 * line `crates/uf_config/tests/flow_schema.rs` draws.
 */
export const OPEN_MAPS: $ReadOnlyArray<string> = [
  "lint.rules",
  "tasks",
  "build.hooks",
  "pm.scopes",
  "env.toolchain",
  "vite",
];

/** Every key path in a serialized config: a leaf is anything that is not a non-empty object. */
export function keyPaths(value: mixed, prefix: string = ""): Array<string> {
  if (OPEN_MAPS.includes(prefix)) {
    return [prefix];
  }
  if (value != null && typeof value === "object" && !Array.isArray(value)) {
    const keys = Object.keys(value);
    if (keys.length > 0) {
      return keys.flatMap((key) => keyPaths(value[key], prefix === "" ? key : `${prefix}.${key}`));
    }
  }
  return prefix === "" ? [] : [prefix];
}

/** Whether the reference names `key`, or an ancestor of it as `ancestor.*`. */
export function keyDocumented(
  named: {| readonly exact: $ReadOnlySet<string>, readonly prefixes: $ReadOnlyArray<string> |},
  key: string,
): boolean {
  return named.exact.has(key) || named.prefixes.some((prefix) => key.startsWith(prefix));
}

/** Every gap, one line each, sorted: `command uf x`, `flag uf x --y`, `config a.b`, `package @s/n`. */
export function gaps(inputs: {|
  readonly commands: $ReadOnlyMap<string, $ReadOnlyArray<string>>,
  readonly keys: $ReadOnlyArray<string>,
  readonly packages: $ReadOnlyArray<string>,
  readonly cliPage: string,
  readonly configPage: string,
  readonly packagesPage: string,
|}): Array<string> {
  const out = [];
  const cli = sections(inputs.cliPage);
  const globals = cli.find((section) => section.heading === "Global flags")?.body ?? "";
  for (const flag of GLOBAL_FLAGS) {
    if (!globals.includes(flag)) {
      out.push(`flag uf ${flag}`);
    }
  }
  for (const [command, flags] of inputs.commands) {
    const found = commandSections(cli, command);
    if (found.length === 0) {
      out.push(`command uf ${command}`);
      continue;
    }
    const body = found.map((section) => section.body).join("\n");
    for (const flag of flags) {
      if (!GLOBAL_FLAGS.includes(flag) && !standsAlone(body, flag)) {
        out.push(`flag uf ${command} ${flag}`);
      }
    }
  }
  const named = namedKeys(inputs.configPage);
  for (const key of inputs.keys) {
    if (!keyDocumented(named, key)) {
      out.push(`config ${key}`);
    }
  }
  for (const name of inputs.packages) {
    if (!standsAlone(inputs.packagesPage, name)) {
      out.push(`package ${name}`);
    }
  }
  return [...new Set(out)].sort();
}

/** What changed against the known list: gaps it does not have, and lines that are no longer gaps. */
export function compare(
  known: $ReadOnlyArray<string>,
  found: $ReadOnlyArray<string>,
): {| readonly added: Array<string>, readonly closed: Array<string> |} {
  const knownSet = new Set(known);
  const foundSet = new Set(found);
  return {
    added: found.filter((line) => !knownSet.has(line)),
    closed: known.filter((line) => !foundSet.has(line)),
  };
}

/** The lines of the known-gaps file that are gaps, not comments or blanks. */
export function readKnown(text: string): Array<string> {
  return text
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line !== "" && !line.startsWith("#"));
}

function help(uf: string, command: $ReadOnlyArray<string>): string {
  return execFileSync(uf, [...command, "--help"], { encoding: "utf8", env: noColor() });
}

function noColor(): { [string]: ?string } {
  return { ...process.env, NO_COLOR: "1", CLICOLOR: "0" };
}

/** Every command below `uf`, with the long flags it takes. */
function commandTree(uf: string): Map<string, $ReadOnlyArray<string>> {
  const tree = new Map<string, $ReadOnlyArray<string>>();
  const walk = (command: Array<string>) => {
    const screen = parseHelp(help(uf, command));
    if (command.length > 0) {
      tree.set(command.join(" "), screen.flags);
    }
    for (const sub of screen.subcommands) {
      walk([...command, sub]);
    }
  };
  walk([]);
  return tree;
}

function configKeys(uf: string): Array<string> {
  const empty = fs.mkdtempSync(path.join(os.tmpdir(), "uf-docs-coverage-"));
  try {
    const out = execFileSync(uf, ["inspect", "--json", "--cwd", empty], {
      encoding: "utf8",
      env: noColor(),
    });
    const parsed: mixed = JSON.parse(out);
    const config =
      parsed != null && typeof parsed === "object" && !Array.isArray(parsed) ? parsed.config : null;
    const inner =
      config != null && typeof config === "object" && !Array.isArray(config) ? config.config : null;
    if (inner == null) {
      throw new Error("uf inspect --json printed no config.config");
    }
    return keyPaths(inner);
  } finally {
    fs.rmSync(empty, { recursive: true, force: true });
  }
}

function publishedPackages(): Array<string> {
  const names = [];
  for (const dir of fs.readdirSync(path.join(REPO, "packages")).sort()) {
    const manifest = path.join(REPO, "packages", dir, "package.json");
    if (!fs.existsSync(manifest)) {
      continue;
    }
    const parsed: mixed = JSON.parse(fs.readFileSync(manifest, "utf8"));
    if (parsed == null || typeof parsed !== "object" || Array.isArray(parsed)) {
      continue;
    }
    if (parsed.private !== true && typeof parsed.name === "string") {
      names.push(parsed.name);
    }
  }
  return names;
}

function read(relative: string): string {
  return fs.readFileSync(path.join(REPO, relative), "utf8");
}

const HEADER = `# What uf ships that the manual does not document yet.
#
# Written by \`tools/docs/coverage.js --update\` and checked by \`uf run docs:coverage\`.
# A gap that is not listed here fails the check, and so does a line here that
# is no longer a gap: document the thing, then delete its line (or rerun with
# --update). The list only shrinks. See tools/docs/coverage.js.
`;

function main(argv: $ReadOnlyArray<string>): void {
  const uf = process.env.UF_BIN ?? "uf";
  const found = gaps({
    commands: commandTree(uf),
    keys: configKeys(uf),
    packages: publishedPackages(),
    cliPage: read(CLI_PAGE),
    configPage: read(CONFIG_PAGE),
    packagesPage: read(PACKAGES_PAGE),
  });

  if (argv.includes("--update")) {
    fs.writeFileSync(GAPS, `${HEADER}\n${found.join("\n")}\n`);
    process.stdout.write(`docs-coverage: wrote ${found.length} known gaps\n`);
    return;
  }

  const known = fs.existsSync(GAPS) ? readKnown(fs.readFileSync(GAPS, "utf8")) : [];
  const { added, closed } = compare(known, found);
  const counts = (prefix: string) => found.filter((line) => line.startsWith(prefix)).length;
  process.stdout.write(
    `docs-coverage: ${found.length} known gaps — ${counts("command ")} commands, ${counts("flag ")} flags, ${counts("config ")} config keys, ${counts("package ")} packages\n`,
  );
  if (added.length > 0) {
    process.stdout.write(
      `\nThese shipped without documentation. Document each where a reader would look it up — ${CLI_PAGE}, ${CONFIG_PAGE} or ${PACKAGES_PAGE}:\n${added.map((line) => `  + ${line}`).join("\n")}\n`,
    );
  }
  if (closed.length > 0) {
    process.stdout.write(
      `\nThese are documented now. Delete their lines from tools/docs/coverage-gaps.txt:\n${closed.map((line) => `  - ${line}`).join("\n")}\n`,
    );
  }
  if (added.length > 0 || closed.length > 0) {
    process.exitCode = 1;
  }
}

if (process.argv[1] != null && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main(process.argv.slice(2));
}
