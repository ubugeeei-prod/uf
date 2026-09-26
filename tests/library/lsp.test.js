// @flow
//
// `uf lsp`, driven the way an editor drives it.
//
// Every editor integration in `editors/` is a promise about what the server
// answers, and a README is a bad place to keep a promise. This drives the real
// binary over the real wire — `Content-Length` frames on stdin, frames back on
// stdout — and asserts each capability the integrations claim. A capability
// that cannot be shown here should not be in any of those READMEs.
//
// The binary is the one running this suite: `uf test` puts its own path in
// `UF_BINARY` so a worker transforms through the build that started it, and
// the same guarantee is what makes this test about *this* checkout rather than
// about whatever `uf` happens to be on PATH.
//
// Messages are written in one go and the answers read back afterwards. The
// server is a synchronous loop over the frames it is given, so this is the
// whole conversation, in order — the same shape `crates/uf_cli/tests/cli.rs`
// uses.

import { spawn, spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

const UF: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error(
      "UF_BINARY is not set: this test drives `uf lsp`, and `uf test` is what names the binary",
    );
  }
  return binary;
})();

const URI = "file:///project/a.js";

// What goes out. A request or a notification is built here rather than typed,
// because half the point of some of these tests is to send something the
// protocol does not describe.
type Message = { [string]: mixed };

// What comes back, written out because a test that navigates a reply should
// say what shape it expects it to have. These follow the protocol and
// `crates/uf_cli/src/commands/dev.rs`, which is what produces them.
type Position = { line: number, character: number };
type Range = { start: Position, end: Position };
type Diagnostic = {
  range: Range,
  severity: number,
  source: string,
  code: string,
  message: string,
  // Flow's type errors carry every location their message refers to.
  relatedInformation?: Array<{ location: { uri: string, range: Range }, message: string }>,
};

// One entry of a `result` that is a list. `textDocument/formatting` answers
// with `TextEdit`s (`range` and `newText`), `textDocument/codeAction` with
// `CodeAction`s (`title`, `kind`, `edit`), and `textDocument/completion` with
// `CompletionItem`s (`label`, a numeric `kind`, `textEdit`), and
// `textDocument/definition` with `Location`s (`uri`, `range`); the tests tell
// them apart by which fields are set, which is itself part of what they check.
type Entry = {
  uri?: string,
  range?: Range,
  newText?: string,
  title?: string,
  kind?: string | number,
  diagnostics?: Array<Diagnostic>,
  edit?: { changes: { [uri: string]: Array<Entry> } },
  label?: string,
  detail?: string,
  documentation?: { kind: string, value: string },
  textEdit?: Entry,
  filterText?: string,
  sortText?: string,
  // A `DocumentSymbol`.
  name?: string,
  selectionRange?: Range,
  children?: Array<Entry>,
};

// A `result` that is not a list: `initialize`'s, and `hover`'s.
type Answer = {
  serverInfo?: { name: string, version: string },
  capabilities?: {
    textDocumentSync?: number,
    documentFormattingProvider?: boolean,
    hoverProvider?: boolean,
    codeActionProvider?: { codeActionKinds: Array<string> },
    definitionProvider?: boolean,
    typeDefinitionProvider?: boolean,
    renameProvider?: { prepareProvider: boolean },
    completionProvider?: { triggerCharacters: Array<string> },
    referencesProvider?: boolean,
    documentHighlightProvider?: boolean,
    documentSymbolProvider?: boolean,
    // Not advertised; named so a test can say so.
    signatureHelpProvider?: mixed,
    workspaceSymbolProvider?: mixed,
    inlayHintProvider?: mixed,
  },
  contents?: { kind: string, value: string },
  range?: Range,
  // A `Range` itself, which is what `prepareRename` answers with.
  start?: Position,
  end?: Position,
  // A `WorkspaceEdit`, which is what `rename` answers with.
  changes?: { [uri: string]: Array<Entry> },
  // A `CompletionList`, which completion sends instead of a bare list when
  // the list is not finished.
  isIncomplete?: boolean,
  items?: Array<Entry>,
};

// One `Location`: what `definition` and `typeDefinition` answer with a list of.
type Location = { uri: string, range: Range };

type Wire = {
  jsonrpc: string,
  id?: number,
  method?: string,
  result?: Answer | Array<Entry> | null,
  error?: { code: number, message: string },
  params?: { uri: string, diagnostics: Array<Diagnostic> },
};

/** One `Content-Length`-framed message, the way an editor sends it. */
const framed = (message: Message): string => {
  const body = JSON.stringify(message);
  return `Content-Length: ${Buffer.byteLength(body, "utf8")}\r\n\r\n${body}`;
};

/**
 * Run one conversation against `uf lsp` and parse everything it said.
 *
 * `cwd` matters: the server reads `uf.config.js` from its working directory,
 * once, at start-up, unless `args` names another with `--cwd`.
 */
const session = (
  messages: Array<Message>,
  cwd: string = process.cwd(),
  env: { [string]: string } = {},
  args: Array<string> = [],
): Array<Wire> => {
  const run = spawnSync(UF, ["lsp", ...args], {
    input: messages.map(framed).join(""),
    cwd,
    env: { ...process.env, ...env },
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (run.status !== 0) {
    throw new Error(`uf lsp exited with ${String(run.status)}: ${String(run.stderr)}`);
  }

  // The one place a shape is asserted rather than proven: JSON has no type,
  // and `JSON.parse` is where it acquires one. Everything below reads the
  // declared shapes instead of casting again.
  const parsed: Array<Wire> = [];
  let rest: string = run.stdout;
  const header = "Content-Length: ";
  while (rest.includes(header)) {
    const after = rest.slice(rest.indexOf(header) + header.length);
    const split = after.indexOf("\r\n\r\n");
    if (split < 0) {
      throw new Error(`an unterminated frame in:\n${run.stdout}`);
    }
    const length = Number.parseInt(after.slice(0, split).trim(), 10);
    const body = after.slice(split + 4);
    parsed.push(JSON.parse(body.slice(0, length)));
    rest = body.slice(length);
  }
  return parsed;
};

const didOpen = (text: string, uri: string = URI): Message => ({
  jsonrpc: "2.0",
  method: "textDocument/didOpen",
  params: { textDocument: { uri, languageId: "javascript", version: 1, text } },
});

const didChange = (text: string): Message => ({
  jsonrpc: "2.0",
  method: "textDocument/didChange",
  params: { textDocument: { uri: URI, version: 2 }, contentChanges: [{ text }] },
});

const EXIT: Message = { jsonrpc: "2.0", method: "exit" };

/** The answer to one request id. */
const answer = (messages: Array<Wire>, id: number): Wire => {
  const found = messages.find((message) => message.id === id);
  if (found == null) {
    throw new Error(`no answer for id ${id} in ${JSON.stringify(messages)}`);
  }
  return found;
};

/**
 * `value`, which the protocol makes optional and this reply has to carry, or a
 * failure that names what was missing. The reply types say what *may* be
 * there; a test that reads a field is also the assertion that it *is*.
 */
function present<T>(value: ?T, what: string): T {
  if (value == null) {
    throw new Error(`the reply has no ${what}`);
  }
  return value;
}

/** One request's `result`, as the object shape `initialize` and `hover` send. */
const answered = (messages: Array<Wire>, id: number): Answer => {
  const result = answer(messages, id).result;
  if (result == null || Array.isArray(result)) {
    throw new Error(
      `expected an object result for id ${id}, got ${String(JSON.stringify(result))}`,
    );
  }
  return result;
};

/** One request's `result`, as the list `formatting` and `codeAction` send. */
const listed = (messages: Array<Wire>, id: number): Array<Entry> => {
  const result = answer(messages, id).result;
  if (!Array.isArray(result)) {
    throw new Error(`expected a list result for id ${id}, got ${String(JSON.stringify(result))}`);
  }
  return result;
};

/** Every `publishDiagnostics` the server pushed, in order. */
const published = (messages: Array<Wire>): Array<Array<Diagnostic>> =>
  messages
    .filter((message) => message.method === "textDocument/publishDiagnostics")
    .map((message) => message.params?.diagnostics ?? []);

/**
 * Apply LSP `TextEdit`s the way an editor applies them.
 *
 * Back to front, so the offsets of the earlier edits stay true. `character` is
 * a UTF-16 code unit and a JavaScript string is indexed in UTF-16 code units,
 * so the two need no conversion — which is exactly the arithmetic the server
 * has to get right on its side.
 */
const apply = (source: string, edits: Array<Entry>): string => {
  const lines = source.split("\n");
  const ordered = edits
    .map((edit) => ({
      range: present(edit.range, "edit range"),
      newText: present(edit.newText, "edit text"),
    }))
    .sort((a, b) => {
      const line = b.range.start.line - a.range.start.line;
      return line !== 0 ? line : b.range.start.character - a.range.start.character;
    });

  for (const edit of ordered) {
    const startLine = edit.range.start.line;
    const endLine = edit.range.end.line;
    const head = lines[startLine].slice(0, edit.range.start.character);
    if (endLine >= lines.length) {
      // A whole-document edit, whose end is past the last line.
      lines.splice(startLine, lines.length - startLine, head + edit.newText);
      continue;
    }
    const tail = lines[endLine].slice(edit.range.end.character);
    lines.splice(startLine, endLine - startLine + 1, head + edit.newText + tail);
  }
  return lines.join("\n");
};

describe("what uf lsp tells an editor it can do", () => {
  it("advertises exactly the capabilities the editor integrations wire up", () => {
    const messages = session([
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { capabilities: {} } },
      EXIT,
    ]);
    const result = answered(messages, 1);

    expect(result.serverInfo?.name).toBe("uf-lsp");
    // Full-document sync: every integration in `editors/` sends whole
    // documents, and an incremental client would be sending edits the server
    // does not read.
    const capabilities = present(result.capabilities, "capabilities");
    expect(capabilities.textDocumentSync).toBe(1);
    expect(capabilities.documentFormattingProvider).toBe(true);
    expect(capabilities.hoverProvider).toBe(true);
    expect(present(capabilities.codeActionProvider, "codeActionProvider").codeActionKinds).toEqual([
      "quickfix",
      "source.fixAll.uf",
    ]);
    // Completion. `"` opens a value or a quoted key in `uf.config.js`; `@`
    // separates a tool from its version; `.` is a member access, whose members
    // Flow's inference knows.
    expect(capabilities.completionProvider?.triggerCharacters).toEqual(['"', "@", "."]);
    // All answered by the checker and Flow's services; see "types" below.
    expect(capabilities.definitionProvider).toBe(true);
    expect(capabilities.typeDefinitionProvider).toBe(true);
    expect(capabilities.referencesProvider).toBe(true);
    expect(capabilities.documentHighlightProvider).toBe(true);
    expect(capabilities.renameProvider).toEqual({ prepareProvider: true });
    expect(capabilities.documentSymbolProvider).toBe(true);
  });

  it("does not advertise what it cannot do", () => {
    // The READMEs are written from this list. `source.organizeImports` is
    // absent because uf has no import-order opinion; signature help and
    // workspace symbols because nothing serves them yet.
    const messages = session([
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { capabilities: {} } },
      EXIT,
    ]);
    const capabilities = answered(messages, 1).capabilities ?? {};

    expect(capabilities.signatureHelpProvider).toBe(undefined);
    expect(capabilities.workspaceSymbolProvider).toBe(undefined);
    expect(capabilities.inlayHintProvider).toBe(undefined);
    expect(
      present(capabilities.codeActionProvider, "codeActionProvider").codeActionKinds,
    ).not.toContain("source.organizeImports");
  });

  it("answers a request it does not serve instead of leaving the editor waiting", () => {
    const messages = session([
      { jsonrpc: "2.0", id: 2, method: "textDocument/signatureHelp", params: {} },
      EXIT,
    ]);

    expect(answer(messages, 2).error?.code).toBe(-32601);
  });
});

describe("diagnostics", () => {
  const SOURCE = "// @flow\ntype B = bool;\n";

  it("pushes them when a document opens, and again when it changes", () => {
    const messages = session([didOpen(SOURCE), didChange(`${SOURCE}type C = bool;\n`), EXIT]);
    const pushes = published(messages);

    expect(pushes).toHaveLength(2);
    expect(pushes[0]).toHaveLength(1);
    expect(pushes[0][0].code).toBe("flow/deprecated-type");
    expect(pushes[0][0].source).toBe("uf");
    // Severity 1 is the protocol's `Error`, which is what `uf lint` calls it.
    expect(pushes[0][0].severity).toBe(1);
    expect(pushes[1]).toHaveLength(2);
  });

  it("clears them when the document closes", () => {
    // An empty list, not silence: an editor keeps the markers it was last
    // given until it is told otherwise.
    const messages = session([
      didOpen(SOURCE),
      { jsonrpc: "2.0", method: "textDocument/didClose", params: { textDocument: { uri: URI } } },
      EXIT,
    ]);

    expect(published(messages)[1]).toEqual([]);
  });

  it("puts the range on the offending token, in UTF-16 units", () => {
    // A character outside the Basic Multilingual Plane, earlier on the same line:
    // the linter counts bytes and the protocol counts UTF-16 code units, and a
    // disagreement is a squiggle in the wrong place. `🦀` is four bytes and two
    // UTF-16 units, so `bool` starts at byte 27 and at unit 25.
    const source = '// @flow\nconst s = "🦀"; type B = bool;\n';
    const messages = session([didOpen(source), EXIT]);
    const range = published(messages)[0][0].range;

    expect(source.split("\n")[1].slice(range.start.character, range.end.character)).toBe("bool");
    expect(range.start).toEqual({ line: 1, character: 25 });
    expect(range.end).toEqual({ line: 1, character: 29 });
  });
});

describe("formatting", () => {
  const format = (id: number): Message => ({
    jsonrpc: "2.0",
    id,
    method: "textDocument/formatting",
    params: { textDocument: { uri: URI }, options: { tabSize: 2, insertSpaces: true } },
  });

  it("returns one edit over the whole document", () => {
    const source = "// @flow\nconst x = {a:1,   b:2};\n";
    const messages = session([didOpen(source), format(3), EXIT]);
    const edits = listed(messages, 3);

    expect(edits).toHaveLength(1);
    expect(apply(source, edits)).toBe("// @flow\nconst x = { a: 1, b: 2 };\n");
  });

  it("returns no edits for a document that is already formatted", () => {
    const source = "// @flow\nconst x = 1;\n";
    const messages = session([didOpen(source), format(3), EXIT]);

    expect(listed(messages, 3)).toEqual([]);
  });

  it("leaves a document it cannot parse alone rather than failing", () => {
    // Half a keystroke into an edit, a document is often not a program yet. An
    // error here would be a popup on every keystroke.
    const messages = session([didOpen("// @flow\nconst = ;\n"), format(3), EXIT]);

    expect(answer(messages, 3).result).toBe(null);
  });

  it("formats to the uf.config.js in the directory it was started in", () => {
    // This is why every integration in `editors/` starts the server *in* the
    // project rather than passing it a path: `uf lsp` reads its configuration
    // from its working directory, and there is no request that can tell it
    // otherwise.
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-cwd-"));
    const elsewhere = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-none-"));
    try {
      fs.writeFileSync(
        path.join(project, "uf.config.js"),
        "// @flow\nexport default { fmt: { indentWidth: 8 } };\n",
      );
      const source = "// @flow\nfunction f() {\nreturn 1;\n}\n";
      const inProject = listed(session([didOpen(source), format(3), EXIT], project), 3);
      const outside = listed(session([didOpen(source), format(3), EXIT], elsewhere), 3);

      expect(apply(source, inProject)).toContain("\n        return 1;");
      expect(apply(source, outside)).toContain("\n  return 1;");
    } finally {
      fs.rmSync(project, { recursive: true, force: true });
      fs.rmSync(elsewhere, { recursive: true, force: true });
    }
  });

  it("formats to the uf.config.js in the directory --cwd names, wherever it was started", () => {
    // What the Zed extension, `uf.vim` and the JetBrains template rely on:
    // they name the project on the command line instead of, or as well as,
    // starting the server in it.
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-flag-"));
    const elsewhere = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-none-"));
    try {
      fs.writeFileSync(
        path.join(project, "uf.config.js"),
        "// @flow\nexport default { fmt: { indentWidth: 8 } };\n",
      );
      const source = "// @flow\nfunction f() {\nreturn 1;\n}\n";
      const named = listed(
        session([didOpen(source), format(3), EXIT], elsewhere, {}, ["--cwd", project]),
        3,
      );

      expect(apply(source, named)).toContain("\n        return 1;");
    } finally {
      fs.rmSync(project, { recursive: true, force: true });
      fs.rmSync(elsewhere, { recursive: true, force: true });
    }
  });
});

describe("code actions", () => {
  const codeAction = (id: number, line: number, only: Array<string> | null): Message => ({
    jsonrpc: "2.0",
    id,
    method: "textDocument/codeAction",
    params: {
      textDocument: { uri: URI },
      range: { start: { line, character: 9 }, end: { line, character: 9 } },
      context: only == null ? { diagnostics: [] } : { diagnostics: [], only },
    },
  });

  it("offers a quick fix whose edit leaves nothing to report", () => {
    const source = "// @flow\ntype B = bool;\n";
    const messages = session([didOpen(source), codeAction(4, 1, null), EXIT]);
    const actions = listed(messages, 4);
    const fix = actions.find((action) => action.kind === "quickfix") ?? {};

    expect(fix.title).toBe("Replace `bool` with `boolean`");
    // The action carries the diagnostic it answers, so the editor can attach
    // it to the right squiggle.
    expect(present(fix.diagnostics, "diagnostics")[0].code).toBe("flow/deprecated-type");

    const fixed = apply(source, present(fix.edit, "edit").changes[URI]);
    expect(fixed).toBe("// @flow\ntype B = boolean;\n");
    // And the linter agrees, because it is the one asked.
    expect(published(session([didOpen(fixed), EXIT]))[0]).toEqual([]);
  });

  it("answers `source.fixAll` with the fix-all action editors save with", () => {
    // This is the exact request VS Code sends for
    // `editor.codeActionsOnSave: { "source.fixAll": "explicit" }`, and Helix
    // and Neovim send the same thing for their own fix-all bindings. Kinds are
    // hierarchical, so asking for the parent has to select uf's child kind.
    const source = "// @flow\ntype A = bool;\ntype B = ?bool;\n";
    const messages = session([didOpen(source), codeAction(4, 1, ["source.fixAll"]), EXIT]);
    const actions = listed(messages, 4);

    expect(actions).toHaveLength(1);
    expect(actions[0].kind).toBe("source.fixAll.uf");
    // Every occurrence in the file, not only the one under the cursor.
    expect(apply(source, present(actions[0].edit, "edit").changes[URI])).toBe(
      "// @flow\ntype A = boolean;\ntype B = ?boolean;\n",
    );
  });

  it("answers the fully spelled kind as well as the parent", () => {
    const source = "// @flow\ntype A = bool;\n";
    const messages = session([didOpen(source), codeAction(4, 1, ["source.fixAll.uf"]), EXIT]);

    expect(listed(messages, 4)[0].kind).toBe("source.fixAll.uf");
  });

  it("offers nothing for a rule whose answer would be a guess", () => {
    const messages = session([didOpen("// @flow\ntype A = any;\n"), codeAction(4, 1, null), EXIT]);

    expect(published(messages)[0][0].code).toBe("flow/unclear-type");
    expect(listed(messages, 4)).toEqual([]);
  });

  it("has nothing to offer for a document it was never sent", () => {
    const messages = session([codeAction(4, 1, null), EXIT]);

    expect(listed(messages, 4)).toEqual([]);
  });
});

describe("hover", () => {
  const hover = (id: number, line: number, character: number): Message => ({
    jsonrpc: "2.0",
    id,
    method: "textDocument/hover",
    params: { textDocument: { uri: URI }, position: { line, character } },
  });

  it("says which rule is behind a diagnostic", () => {
    const messages = session([didOpen("// @flow\ntype B = bool;\n"), hover(5, 1, 9), EXIT]);
    const result = answered(messages, 5);

    expect(result.contents?.kind).toBe("markdown");
    expect(result.contents?.value).toContain("flow/deprecated-type");
    expect(result.range).toEqual({
      start: { line: 1, character: 9 },
      end: { line: 1, character: 13 },
    });
  });

  it("says what a `@uniflowed/*` import names", () => {
    const source = '// @flow\nimport { effect } from "@uniflowed/effect";\n';
    const messages = session([didOpen(source), hover(5, 1, 26), EXIT]);

    expect(answered(messages, 5).contents?.value).toContain("@uniflowed/effect");
  });

  it("answers the type of an expression from Flow's inference", () => {
    // Everything else about types is in "types" below; this is the one that
    // used to be `null`, in the document every other hover test uses.
    // In an empty directory: the server reads the project it was started in
    // to answer a type, and this one should not be uf's own repository.
    const empty = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-hover-"));
    try {
      const messages = session([didOpen("// @flow\nconst x = 1;\n"), hover(5, 1, 6), EXIT], empty);

      expect(answered(messages, 5).contents?.value).toBe("```flow\nconst x: 1\n```");
    } finally {
      fs.rmSync(empty, { recursive: true, force: true });
    }
  });
});

describe("types", () => {
  // A project on disk, because the server reads the project the way `uf check`
  // does: a scan of its root, the modules those files import — including a
  // package under `node_modules` — and the library definitions in
  // `flow-typed/`. Real paths, because the server's root is its working
  // directory as the file system spells it.
  const FILES: { [string]: string } = {
    "src/user.js": [
      "// @flow",
      "export type User = { name: string, age: number };",
      "export function greet(user: User): string {",
      "  return user.name;",
      "}",
      "",
    ].join("\n"),
    "src/app.js": [
      "// @flow",
      "import { greet, type User } from './user.js';",
      "import { tick } from 'clock';",
      "const user: User = { name: 'Ada', age: 36 };",
      "const greeting = greet(user);",
      "const now = tick();",
      "const stamped = stamp();",
      "",
    ].join("\n"),
    "node_modules/clock/package.json": JSON.stringify({ name: "clock", main: "index.js" }),
    "node_modules/clock/index.js": "exports.tick = () => 0;\n",
    "node_modules/clock/index.js.flow": "// @flow\ndeclare export function tick(): number;\n",
    "flow-typed/stamp.js": "declare function stamp(): string;\n",
  };

  const withProject = (run: (root: string, uri: (file: string) => string) => void): void => {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-types-")));
    try {
      for (const [file, text] of Object.entries(FILES)) {
        fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
        fs.writeFileSync(path.join(root, file), String(text));
      }
      run(root, (file) => `file://${path.join(root, file)}`);
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  };

  const at = (
    id: number,
    method: string,
    uri: string,
    line: number,
    character: number,
  ): Message => ({
    jsonrpc: "2.0",
    id,
    method,
    params: { textDocument: { uri }, position: { line, character } },
  });

  const open = (uri: string, text: string): Message => didOpen(text, uri);

  const change = (uri: string, text: string): Message => ({
    jsonrpc: "2.0",
    method: "textDocument/didChange",
    params: { textDocument: { uri, version: 2 }, contentChanges: [{ text }] },
  });

  const INITIALIZED: Message = { jsonrpc: "2.0", method: "initialized", params: {} };

  /** One request's `result`, as the list of `Location`s a definition sends. */
  const locations = (messages: Array<Wire>, id: number): Array<Location> =>
    listed(messages, id).map((entry) => {
      const { uri, range } = entry;
      if (uri == null || range == null) {
        throw new Error(`expected a Location for id ${id}, got ${JSON.stringify(entry)}`);
      }
      return { uri, range };
    });

  const app = FILES["src/app.js"];

  it("hovers the inferred type of a binding, printed as Flow", () => {
    withProject((root, uri) => {
      const messages = session(
        [
          INITIALIZED,
          open(uri("src/app.js"), app),
          at(1, "textDocument/hover", uri("src/app.js"), 4, 8),
          EXIT,
        ],
        root,
      );
      const result = answered(messages, 1);

      expect(result.contents).toEqual({
        kind: "markdown",
        value: "```flow\nconst greeting: string\n```",
      });
      expect(result.range).toEqual({
        start: { line: 4, character: 6 },
        end: { line: 4, character: 14 },
      });
    });
  });

  it("hovers a name imported from another file with the type that file gives it", () => {
    withProject((root, uri) => {
      const messages = session(
        [open(uri("src/app.js"), app), at(1, "textDocument/hover", uri("src/app.js"), 4, 18), EXIT],
        root,
      );

      expect(answered(messages, 1).contents?.value).toContain("(user: User) => string");
    });
  });

  it("hovers a package's export with the type its `.flow` file declares", () => {
    withProject((root, uri) => {
      const messages = session(
        [open(uri("src/app.js"), app), at(1, "textDocument/hover", uri("src/app.js"), 5, 7), EXIT],
        root,
      );

      expect(answered(messages, 1).contents?.value).toBe("```flow\nconst now: number\n```");
    });
  });

  it("answers the text the editor holds, not the file on disk", () => {
    withProject((root, uri) => {
      const edited = app.replace("const greeting = greet(user);", "const greeting = user.age;");
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/hover", uri("src/app.js"), 4, 8),
          change(uri("src/app.js"), edited),
          at(2, "textDocument/hover", uri("src/app.js"), 4, 8),
          EXIT,
        ],
        root,
      );

      expect(answered(messages, 1).contents?.value).toContain("const greeting: string");
      expect(answered(messages, 2).contents?.value).toContain("const greeting: number");
    });
  });

  it("sees an edit to a file another file imports", () => {
    withProject((root, uri) => {
      const user = FILES["src/user.js"]
        .replace("): string {", "): number {")
        .replace("return user.name;", "return user.age;");
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/hover", uri("src/app.js"), 4, 8),
          open(uri("src/user.js"), user),
          at(2, "textDocument/hover", uri("src/app.js"), 4, 8),
          EXIT,
        ],
        root,
      );

      expect(answered(messages, 1).contents?.value).toContain("const greeting: string");
      expect(answered(messages, 2).contents?.value).toContain("const greeting: number");
    });
  });

  it("says nothing where nothing is typed", () => {
    withProject((root, uri) => {
      // Inside the `// @flow` comment.
      const messages = session(
        [open(uri("src/app.js"), app), at(1, "textDocument/hover", uri("src/app.js"), 0, 4), EXIT],
        root,
      );

      expect(answer(messages, 1).result).toBe(null);
    });
  });

  it("goes to a definition in another file", () => {
    withProject((root, uri) => {
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/definition", uri("src/app.js"), 4, 18),
          EXIT,
        ],
        root,
      );
      const found = locations(messages, 1);

      expect(found).toHaveLength(1);
      expect(found[0].uri).toBe(uri("src/user.js"));
      // `greet`, on the third line of `src/user.js`.
      expect(found[0].range.start).toEqual({ line: 2, character: 16 });
    });
  });

  it("goes to a definition in a package under node_modules", () => {
    withProject((root, uri) => {
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/definition", uri("src/app.js"), 5, 13),
          EXIT,
        ],
        root,
      );
      const found = locations(messages, 1);

      expect(found).toHaveLength(1);
      expect(found[0].uri).toBe(uri("node_modules/clock/index.js.flow"));
      expect(found[0].range.start.line).toBe(1);
    });
  });

  it("goes to a definition in the project's library definitions", () => {
    withProject((root, uri) => {
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/definition", uri("src/app.js"), 6, 17),
          EXIT,
        ],
        root,
      );
      const found = locations(messages, 1);

      expect(found).toHaveLength(1);
      expect(found[0].uri).toBe(uri("flow-typed/stamp.js"));
    });
  });

  it("goes to the declaration of a binding's type", () => {
    withProject((root, uri) => {
      // `user`, declared as a `User`.
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/typeDefinition", uri("src/app.js"), 3, 7),
          EXIT,
        ],
        root,
      );
      const found = locations(messages, 1);

      expect(found).toHaveLength(1);
      expect(found[0].uri).toBe(uri("src/user.js"));
      expect(found[0].range.start.line).toBe(1);
    });
  });

  it("completes the members of a value's type, with their types", () => {
    withProject((root, uri) => {
      const typing = `${app}user.`;
      const lines = typing.split("\n");
      const messages = session(
        [
          open(uri("src/app.js"), typing),
          at(1, "textDocument/completion", uri("src/app.js"), lines.length - 1, 5),
          EXIT,
        ],
        root,
      );
      const result = answered(messages, 1);
      const offered = (result.items ?? []).map((item) => [item.label, item.detail]);

      expect(offered).toContainEqual(["age", "number"]);
      expect(offered).toContainEqual(["name", "string"]);
      // `user.` offers what `User` has, and nothing that is merely in scope.
      expect(offered.map(([label]) => label)).not.toContain("greeting");
    });
  });

  it("finds every reference to a name, across the files that import it", () => {
    withProject((root, uri) => {
      const references = (id: number, includeDeclaration: boolean): Message => ({
        jsonrpc: "2.0",
        id,
        method: "textDocument/references",
        params: {
          textDocument: { uri: uri("src/app.js") },
          // `greet`, where `src/app.js` calls it.
          position: { line: 4, character: 18 },
          context: { includeDeclaration },
        },
      });
      const messages = session(
        [open(uri("src/app.js"), app), references(1, true), references(2, false), EXIT],
        root,
      );
      const where = (id: number): Array<[string, number, number, number]> =>
        locations(messages, id).map(({ uri: file, range }) => [
          path.relative(root, file.replace("file://", "")),
          range.start.line,
          range.start.character,
          range.end.character,
        ]);

      expect(where(1)).toEqual([
        ["src/app.js", 1, 9, 14],
        ["src/app.js", 4, 17, 22],
        ["src/user.js", 2, 16, 21],
      ]);
      // Without the declaration, which is the one in `src/user.js`.
      expect(where(2)).toEqual([
        ["src/app.js", 1, 9, 14],
        ["src/app.js", 4, 17, 22],
      ]);
    });
  });

  it("highlights every use of a name in the document", () => {
    withProject((root, uri) => {
      const messages = session(
        // `user`, where it is declared.
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/documentHighlight", uri("src/app.js"), 3, 7),
          EXIT,
        ],
        root,
      );

      expect(listed(messages, 1)).toEqual([
        { range: { start: { line: 3, character: 6 }, end: { line: 3, character: 10 } }, kind: 1 },
        { range: { start: { line: 4, character: 23 }, end: { line: 4, character: 27 } }, kind: 1 },
      ]);
    });
  });

  it("renames a name in every file that writes it", () => {
    withProject((root, uri) => {
      const rename = (id: number, newName: string, line: number, character: number): Message => ({
        jsonrpc: "2.0",
        id,
        method: "textDocument/rename",
        params: {
          textDocument: { uri: uri("src/app.js") },
          position: { line, character },
          newName,
        },
      });
      const messages = session(
        [
          open(uri("src/app.js"), app),
          at(1, "textDocument/prepareRename", uri("src/app.js"), 4, 18),
          rename(2, "welcome", 4, 18),
          // Not an identifier.
          rename(3, "class", 4, 18),
          // `tick`, which a package under `node_modules` declares.
          rename(4, "tock", 5, 13),
          EXIT,
        ],
        root,
      );

      const prepared = answered(messages, 1);
      expect([prepared.start, prepared.end]).toEqual([
        { line: 4, character: 17 },
        { line: 4, character: 22 },
      ]);

      const changes = present(answered(messages, 2).changes, "changes");
      expect(Object.keys(changes).sort()).toEqual([uri("src/app.js"), uri("src/user.js")]);
      const renamedApp = apply(app, changes[uri("src/app.js")]);
      expect(renamedApp).toContain("import { welcome, type User } from './user.js';");
      expect(renamedApp).toContain("const greeting = welcome(user);");
      expect(apply(FILES["src/user.js"], changes[uri("src/user.js")])).toContain(
        "export function welcome(user: User): string {",
      );

      expect(answer(messages, 3).error?.code).toBe(-32602);
      expect(answer(messages, 4).error?.message).toContain("node_modules/clock");
    });
  });

  it("outlines a document, nested as it is written", () => {
    withProject((root, uri) => {
      const source = [
        "// @flow",
        "export class Greeter {",
        "  greet(): string { return 'hi'; }",
        "}",
        "export function make(): Greeter { return new Greeter(); }",
        "",
      ].join("\n");
      const messages = session(
        [
          open(uri("src/greeter.js"), source),
          {
            jsonrpc: "2.0",
            id: 1,
            method: "textDocument/documentSymbol",
            params: { textDocument: { uri: uri("src/greeter.js") } },
          },
          EXIT,
        ],
        root,
      );
      const outline = listed(messages, 1);

      // `SymbolKind`: 5 is a class, 6 a method, 12 a function.
      expect(outline.map((symbol) => [symbol.name, symbol.kind])).toEqual([
        ["Greeter", 5],
        ["make", 12],
      ]);
      const members = present(outline[0].children, "the class's members");
      expect(members.map((symbol) => [symbol.name, symbol.kind])).toEqual([["greet", 6]]);
      expect(members[0].selectionRange).toEqual({
        start: { line: 2, character: 2 },
        end: { line: 2, character: 7 },
      });
    });
  });

  describe("type errors", () => {
    // A server held open the way an editor holds it: type errors are checked
    // in the pauses between messages, so a conversation written in one go
    // never has one. Each step writes, then waits for the push it expects.
    const conversation = async (
      root: string,
      steps: (
        send: (message: Message) => void,
        pushed: (uri: string, found: (Array<Diagnostic>) => boolean) => Promise<Array<Diagnostic>>,
      ) => Promise<void>,
    ): Promise<void> => {
      const child = spawn(UF, ["lsp"], { cwd: root, stdio: ["pipe", "pipe", "inherit"] });
      const closed = new Promise((resolve) => child.on("close", resolve));
      const pushes: Array<{ uri: string, diagnostics: Array<Diagnostic> }> = [];
      let pending = Buffer.alloc(0);
      child.stdout.on("data", (chunk: Buffer) => {
        pending = Buffer.concat([pending, chunk]);
        for (;;) {
          const split = pending.indexOf("\r\n\r\n");
          if (split < 0) {
            return;
          }
          const header = pending.subarray(0, split).toString("utf8");
          const length = Number.parseInt(header.replace(/^Content-Length:\s*/i, ""), 10);
          if (pending.length < split + 4 + length) {
            return;
          }
          const message: Wire = JSON.parse(
            pending.subarray(split + 4, split + 4 + length).toString("utf8"),
          );
          pending = pending.subarray(split + 4 + length);
          if (message.method === "textDocument/publishDiagnostics" && message.params != null) {
            pushes.push(message.params);
          }
        }
      });
      const send = (message: Message): void => {
        child.stdin.write(framed(message));
      };
      // The first push for `uri` after this call that `found` accepts.
      const pushed = async (
        uri: string,
        found: (Array<Diagnostic>) => boolean,
      ): Promise<Array<Diagnostic>> => {
        const from = pushes.length;
        for (let waited = 0; waited < 60_000; waited += 20) {
          const match = pushes
            .slice(from)
            .find((push) => push.uri === uri && found(push.diagnostics));
          if (match != null) {
            return match.diagnostics;
          }
          await new Promise((resolve) => setTimeout(resolve, 20));
        }
        throw new Error(`no matching push for ${uri} in ${JSON.stringify(pushes.slice(from))}`);
      };
      try {
        send({ jsonrpc: "2.0", id: 1, method: "initialize", params: { capabilities: {} } });
        send(INITIALIZED);
        await steps(send, pushed);
      } finally {
        child.stdin.end(framed(EXIT));
        await closed;
      }
    };

    const flow = (diagnostics: Array<Diagnostic>): Array<Diagnostic> =>
      diagnostics.filter((diagnostic) => diagnostic.source === "flow");

    const withProjectAsync = async (
      run: (root: string, uri: (file: string) => string) => Promise<void>,
    ): Promise<void> => {
      const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-errors-")));
      try {
        for (const [file, text] of Object.entries(FILES)) {
          fs.mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
          fs.writeFileSync(path.join(root, file), String(text));
        }
        await run(root, (file) => `file://${path.join(root, file)}`);
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    };

    // `age` given a string where `User` says number, on the fourth line.
    const broken = app.replace("age: 36", "age: '36'");

    it("pushes Flow's type errors beside the linter's, with where each reference points", async () => {
      await withProjectAsync(async (root, uri) => {
        await conversation(root, async (send, pushed) => {
          send(open(uri("src/app.js"), broken));
          const found = flow(
            await pushed(uri("src/app.js"), (diagnostics) => flow(diagnostics).length > 0),
          );

          expect(found).toHaveLength(1);
          const [error] = found;
          expect(error.code).toBe("incompatible-type");
          expect(error.severity).toBe(1);
          // On `'36'`, in UTF-16 units of the line the editor holds.
          const line = broken.split("\n")[error.range.start.line];
          expect(line.slice(error.range.start.character, error.range.end.character)).toBe("'36'");
          // Flow's sentence, numbered the way `uf check` numbers it.
          expect(error.message).toBe(
            'Cannot assign object literal to user because in property age: "36" [1] is incompatible with number [2].',
          );
          // `[2]` is `age: number` in `src/user.js`, a file the editor never opened.
          const related = error.relatedInformation ?? [];
          expect(related.map((entry) => entry.message)).toEqual(['[1] "36"', "[2] number"]);
          expect(related[0].location.uri).toBe(uri("src/app.js"));
          expect(related[1].location.uri).toBe(uri("src/user.js"));
          expect(related[1].location.range.start.line).toBe(1);
        });
      });
    });

    it("clears an error once it is fixed, and finds one an edit to an imported file causes", async () => {
      await withProjectAsync(async (root, uri) => {
        await conversation(root, async (send, pushed) => {
          send(open(uri("src/app.js"), broken));
          await pushed(uri("src/app.js"), (diagnostics) => flow(diagnostics).length > 0);

          send(change(uri("src/app.js"), app));
          await pushed(uri("src/app.js"), (diagnostics) => flow(diagnostics).length === 0);

          // `greet` now wants a number. What breaks is the call in
          // `src/app.js`, which nobody edited — the editor is told anyway.
          const user = FILES["src/user.js"].replace("greet(user: User)", "greet(user: number)");
          send(open(uri("src/user.js"), FILES["src/user.js"]));
          send(change(uri("src/user.js"), user));
          const found = flow(
            await pushed(uri("src/app.js"), (diagnostics) => flow(diagnostics).length > 0),
          );
          expect(found.every((diagnostic) => diagnostic.range.start.line === 4)).toBe(true);
        });
      });
    });

    it("says nothing about types in a file that does not parse, and leaves that to the linter", async () => {
      await withProjectAsync(async (root, uri) => {
        await conversation(root, async (send, pushed) => {
          send(open(uri("src/app.js"), broken));
          await pushed(uri("src/app.js"), (diagnostics) => flow(diagnostics).length > 0);

          send(change(uri("src/app.js"), `${broken}const = ;\n`));
          const found = await pushed(
            uri("src/app.js"),
            (diagnostics) => flow(diagnostics).length === 0,
          );
          expect(found.map((diagnostic) => diagnostic.code)).toContain("flow/syntax");
        });
      });
    });
  });
});

describe("completion in uf.config.js", () => {
  const CONFIG = "file:///project/uf.config.js";

  const complete = (
    id: number,
    line: number,
    character: number,
    uri: string = CONFIG,
  ): Message => ({
    jsonrpc: "2.0",
    id,
    method: "textDocument/completion",
    params: { textDocument: { uri }, position: { line, character } },
  });

  // A document with `‸` where the cursor is: the text without it, and the
  // protocol position of the cursor. A JavaScript string is indexed in UTF-16
  // code units, which is what `character` counts.
  const marked = (document: string): { text: string, line: number, character: number } => {
    const before = document.slice(0, document.indexOf("‸")).split("\n");
    return {
      text: document.replace("‸", ""),
      line: before.length - 1,
      character: before[before.length - 1].length,
    };
  };

  const completeAt = (
    document: string,
    cwd?: string,
    env?: { [string]: string },
  ): { text: string, items: Array<Entry> } => {
    const { text, line, character } = marked(document);
    const messages = session([didOpen(text, CONFIG), complete(9, line, character), EXIT], cwd, env);
    return { text, items: listed(messages, 9) };
  };

  const labels = (items: Array<Entry>): Array<string> => items.map((item) => item.label ?? "");

  // Node's releases, newest first, with a prerelease that is never offered.
  const NODE_RELEASES: Array<{ version: string, date: string, lts?: string }> = [
    { version: "27.0.0-rc.1", date: "2026-10-02" },
    { version: "26.10.0", date: "2026-10-01" },
    { version: "24.14.0", date: "2026-08-20", lts: "Krypton" },
    { version: "22.20.0", date: "2026-06-01", lts: "Jod" },
  ];
  const NODE_VERSIONS = ["26", "24", "22", "26.10.0", "24.14.0", "22.20.0"];

  // Where release lists are cached and fetched from, for one test: a fresh
  // cache — holding Node's list, fetched just now, when `cached` — and a
  // publisher on `file://` serving nodejs.org's `index.json` when `published`,
  // and serving nothing otherwise. Nothing here reaches a network.
  const releaseLists = (options: {
    cached?: boolean,
    published?: boolean,
  }): { env: { [string]: string }, cache: string, cleanup: () => void } => {
    const cache = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-releases-cache-"));
    const publisher = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-releases-publisher-"));
    if (options.cached === true) {
      const index = {
        format: 1,
        tool: "node",
        fetchedAt: Math.floor(Date.now() / 1000),
        sources: ["fixture"],
        releases: NODE_RELEASES,
      };
      fs.writeFileSync(path.join(cache, "node.json"), JSON.stringify(index));
    }
    if (options.published === true) {
      const rows = NODE_RELEASES.map((release) => ({
        version: `v${release.version}`,
        date: release.date,
        files: [] as Array<string>,
        lts: release.lts ?? false,
      }));
      fs.writeFileSync(path.join(publisher, "index.json"), JSON.stringify(rows));
    }
    return {
      env: { UF_INDEX_CACHE: cache, UF_TOOL_INDEX_BASE: `file://${publisher}` },
      cache,
      cleanup: () => {
        fs.rmSync(cache, { recursive: true, force: true });
        fs.rmSync(publisher, { recursive: true, force: true });
      },
    };
  };

  it("completes the tool names a key typed as a runtime takes", () => {
    const lists = releaseLists({});
    try {
      const { text, items } = completeAt(
        'export default defineConfig({ test: { runtime: "‸" } });\n',
        undefined,
        lists.env,
      );

      expect(labels(items)).toEqual(["node", "bun", "deno"]);
      expect(apply(text, [items[1].textEdit ?? {}])).toBe(
        'export default defineConfig({ test: { runtime: "bun" } });\n',
      );
    } finally {
      lists.cleanup();
    }
  });

  it("completes a tool's versions after its `@`, majors first, from the cached release list", () => {
    const lists = releaseLists({ cached: true });
    try {
      const { text, items } = completeAt(
        'export default defineConfig({ runtime: "node@‸" });\n',
        undefined,
        lists.env,
      );

      expect(labels(items)).toEqual(NODE_VERSIONS);
      expect(items[1].kind).toBe(12);
      expect(items[1].detail).toBe("24.14.0 · LTS Krypton");
      expect(apply(text, [items[1].textEdit ?? {}])).toBe(
        'export default defineConfig({ runtime: "node@24" });\n',
      );
    } finally {
      lists.cleanup();
    }
  });

  it("fetches a missing release list behind the request instead of waiting for it", async () => {
    // A server held open the way an editor holds it, because the point is what
    // happens between one request and the next.
    const lists = releaseLists({ published: true });
    const child = spawn(UF, ["lsp"], {
      env: { ...process.env, ...lists.env },
      stdio: ["pipe", "pipe", "inherit"],
    });
    const closed = new Promise((resolve) => child.on("close", resolve));
    const answers: Map<number, Wire> = new Map();
    let pending = Buffer.alloc(0);
    child.stdout.on("data", (chunk: Buffer) => {
      pending = Buffer.concat([pending, chunk]);
      for (;;) {
        const split = pending.indexOf("\r\n\r\n");
        if (split < 0) {
          return;
        }
        const header = pending.subarray(0, split).toString("utf8");
        const length = Number.parseInt(header.replace(/^Content-Length:\s*/i, ""), 10);
        if (pending.length < split + 4 + length) {
          return;
        }
        const message: Wire = JSON.parse(
          pending.subarray(split + 4, split + 4 + length).toString("utf8"),
        );
        pending = pending.subarray(split + 4 + length);
        if (message.id != null) {
          answers.set(message.id, message);
        }
      }
    });
    const pause = (ms: number): Promise<void> => new Promise((resolve) => setTimeout(resolve, ms));

    const text = 'export default defineConfig({ runtime: "node@" });\n';
    const character = text.indexOf("@") + 1;
    const ask = async (id: number): Promise<Wire> => {
      child.stdin.write(framed(complete(id, 0, character)));
      for (let waited = 0; waited < 10_000; waited += 10) {
        const found = answers.get(id);
        if (found != null) {
          return found;
        }
        await pause(10);
      }
      throw new Error(`no answer for id ${id}`);
    };

    try {
      child.stdin.write(framed(didOpen(text, CONFIG)));

      // Answered at once, with nothing yet — and told to ask again.
      expect((await ask(1)).result).toEqual({ isIncomplete: true, items: [] });

      // Asked again as somebody types, until the list has landed.
      let versions: Array<Entry> = [];
      for (let id = 2; id < 500 && versions.length === 0; id += 1) {
        const result = (await ask(id)).result;
        if (Array.isArray(result)) {
          versions = result;
        } else {
          await pause(20);
        }
      }
      expect(labels(versions)).toEqual(NODE_VERSIONS);
      // And it is cached, so the next session does not wait even once.
      expect(fs.existsSync(path.join(lists.cache, "node.json"))).toBe(true);
    } finally {
      child.stdin.end(framed(EXIT));
      await closed;
      lists.cleanup();
    }
  });

  it("completes a half-typed key from the config schema, with its documentation and type", () => {
    // Unclosed and mid-word: the state a document is in when somebody wants a
    // completion, and one the Flow parser cannot read.
    const { text, items } = completeAt("export default defineConfig({\n  test: {\n    cov‸\n");
    const coverage = items.find((item) => item.label === "coverage") ?? {};

    expect(coverage.kind).toBe(10);
    expect(coverage.detail).toBe("{ … }");
    expect(coverage.documentation?.kind).toBe("markdown");
    expect(coverage.documentation?.value).toContain("What `uf test --coverage` measures");
    // The half-typed word is replaced, and the cursor is left where the value goes.
    expect(apply(text, [coverage.textEdit ?? {}])).toBe(
      "export default defineConfig({\n  test: {\n    coverage: \n",
    );
  });

  it("does not offer a key the object already has", () => {
    const { items } = completeAt(
      'export default defineConfig({\n  fmt: { quotes: "single" },\n  ‸\n});\n',
    );

    expect(labels(items)).toContain("lint");
    expect(labels(items)).not.toContain("fmt");
  });

  it("completes the members of a string union inside its quotes", () => {
    const { text, items } = completeAt('export default defineConfig({ fmt: { quotes: "‸" } });\n');

    expect(labels(items)).toEqual(["single", "double"]);
    expect(items[0].kind).toBe(20);
    expect(apply(text, [items[1].textEdit ?? {}])).toBe(
      'export default defineConfig({ fmt: { quotes: "double" } });\n',
    );
  });

  it("writes a value typed without quotes in the project's own quote style", () => {
    // The style comes from the `uf.config.js` the server was started with,
    // which is not the document being edited.
    const project = fs.mkdtempSync(path.join(os.tmpdir(), "uf-lsp-quotes-"));
    try {
      fs.writeFileSync(
        path.join(project, "uf.config.js"),
        '// @flow\nexport default { fmt: { quotes: "single" } };\n',
      );
      const { text, items } = completeAt(
        "export default defineConfig({ fmt: { quotes: ‸ } });\n",
        project,
      );

      expect(labels(items)).toEqual(["'single'", "'double'"]);
      expect(apply(text, [items[0].textEdit ?? {}])).toBe(
        "export default defineConfig({ fmt: { quotes: 'single' } });\n",
      );
    } finally {
      fs.rmSync(project, { recursive: true, force: true });
    }
  });

  it("completes true and false for a boolean, and nothing inside quotes", () => {
    expect(
      labels(completeAt("export default defineConfig({ fmt: { semicolons: ‸ } });\n").items),
    ).toEqual(["true", "false"]);
    expect(
      completeAt('export default defineConfig({ fmt: { semicolons: "‸" } });\n').items,
    ).toEqual([]);
  });

  it("puts the edit in UTF-16 units after a character outside the Basic Multilingual Plane", () => {
    const { text, items } = completeAt(
      'export default defineConfig({ docs: { app: "🦀" }, fmt: { quotes: "‸" } });\n',
    );

    expect(apply(text, [items[0].textEdit ?? {}])).toBe(
      'export default defineConfig({ docs: { app: "🦀" }, fmt: { quotes: "single" } });\n',
    );
  });

  it("answers nothing in a document that is not Flow", () => {
    // The trigger characters fire in every file an editor gives the server,
    // and a stylesheet is not a question uf has an answer to. A Flow file is
    // answered by the checker instead; see "types" below.
    const styles = "file:///project/styles.css";
    const messages = session([
      didOpen("a { color: red; }\n", styles),
      complete(9, 0, 2, styles),
      EXIT,
    ]);

    expect(answer(messages, 9).result).toBe(null);
  });

  it("explains the key under the pointer with the documentation completion shows", () => {
    const text = "export default defineConfig({ test: { coverage: {} } });\n";
    const start = text.indexOf("coverage");
    const messages = session([
      didOpen(text, CONFIG),
      {
        jsonrpc: "2.0",
        id: 9,
        method: "textDocument/hover",
        params: { textDocument: { uri: CONFIG }, position: { line: 0, character: start + 2 } },
      },
      EXIT,
    ]);
    const result = answered(messages, 9);

    expect(result.contents?.value).toContain("**`test.coverage`**");
    expect(result.contents?.value).toContain("What `uf test --coverage` measures");
    expect(result.range).toEqual({
      start: { line: 0, character: start },
      end: { line: 0, character: start + "coverage".length },
    });
  });
});

describe("surviving whatever arrives on the pipe", () => {
  it("answers a body that is not JSON and keeps serving", () => {
    const broken = "{not json";
    const run = spawnSync(UF, ["lsp"], {
      input:
        `Content-Length: ${broken.length}\r\n\r\n${broken}` +
        framed({ jsonrpc: "2.0", id: 6, method: "initialize", params: {} }) +
        framed(EXIT),
      encoding: "utf8",
    });

    expect(run.status).toBe(0);
    expect(run.stdout).toContain("-32700");
    // The next frame was still read, so the stream stayed in sync.
    expect(run.stdout).toContain("uf-lsp");
  });

  it("says nothing to a notification it does not serve", () => {
    const messages = session([
      { jsonrpc: "2.0", method: "textDocument/somethingElse", params: {} },
      { jsonrpc: "2.0", id: 7, method: "initialize", params: {} },
      EXIT,
    ]);

    // Answering a notification is itself a protocol violation, so there is
    // exactly one message back: the answer to the request.
    expect(messages).toHaveLength(1);
    expect(messages[0].id).toBe(7);
  });

  it("refuses the rest of a session after `shutdown`", () => {
    const messages = session([
      { jsonrpc: "2.0", id: 8, method: "shutdown" },
      { jsonrpc: "2.0", id: 9, method: "textDocument/hover", params: {} },
      EXIT,
    ]);

    expect(answer(messages, 8).result).toBe(null);
    expect(answer(messages, 9).error?.code).toBe(-32600);
  });
});
