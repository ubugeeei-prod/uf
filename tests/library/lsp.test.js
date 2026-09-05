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

import { spawnSync } from "node:child_process";
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
};

// One entry of a `result` that is a list. `textDocument/formatting` answers
// with `TextEdit`s (`range` and `newText`) and `textDocument/codeAction` with
// `CodeAction`s (`title`, `kind`, `edit`); the tests tell them apart by which
// fields are set, which is itself part of what they check.
type Entry = {
  range?: Range,
  newText?: string,
  title?: string,
  kind?: string,
  diagnostics?: Array<Diagnostic>,
  edit?: { changes: { [uri: string]: Array<Entry> } },
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
    renameProvider?: boolean,
    completionProvider?: { ... },
    referencesProvider?: boolean,
    documentSymbolProvider?: boolean,
  },
  contents?: { kind: string, value: string },
  range?: Range,
};

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
 * once, at start-up.
 */
const session = (messages: Array<Message>, cwd: string = process.cwd()): Array<Wire> => {
  const run = spawnSync(UF, ["lsp"], {
    input: messages.map(framed).join(""),
    cwd,
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

/** One request's `result`, as the object shape `initialize` and `hover` send. */
const answered = (messages: Array<Wire>, id: number): Answer => {
  const result = answer(messages, id).result;
  if (result == null || Array.isArray(result)) {
    throw new Error(`expected an object result for id ${id}, got ${JSON.stringify(result)}`);
  }
  return result;
};

/** One request's `result`, as the list `formatting` and `codeAction` send. */
const listed = (messages: Array<Wire>, id: number): Array<Entry> => {
  const result = answer(messages, id).result;
  if (!Array.isArray(result)) {
    throw new Error(`expected a list result for id ${id}, got ${JSON.stringify(result)}`);
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
  const ordered = edits.slice().sort((a, b) => {
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
    expect(result.capabilities.textDocumentSync).toBe(1);
    expect(result.capabilities.documentFormattingProvider).toBe(true);
    expect(result.capabilities.hoverProvider).toBe(true);
    expect(result.capabilities.codeActionProvider.codeActionKinds).toEqual([
      "quickfix",
      "source.fixAll.uf",
    ]);
  });

  it("does not advertise what it cannot do", () => {
    // The READMEs are written from this list. `source.organizeImports` is
    // absent because uf has no import-order opinion, and go-to-definition,
    // rename and completion are absent because nothing serves them.
    const messages = session([
      { jsonrpc: "2.0", id: 1, method: "initialize", params: { capabilities: {} } },
      EXIT,
    ]);
    const capabilities = answered(messages, 1).capabilities ?? {};

    expect(capabilities.definitionProvider).toBe(undefined);
    expect(capabilities.renameProvider).toBe(undefined);
    expect(capabilities.completionProvider).toBe(undefined);
    expect(capabilities.referencesProvider).toBe(undefined);
    expect(capabilities.documentSymbolProvider).toBe(undefined);
    expect(capabilities.codeActionProvider.codeActionKinds).not.toContain("source.organizeImports");
  });

  it("answers a request it does not serve instead of leaving the editor waiting", () => {
    const messages = session([
      { jsonrpc: "2.0", id: 2, method: "textDocument/definition", params: {} },
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
    expect(fix.diagnostics[0].code).toBe("flow/deprecated-type");

    const fixed = apply(source, fix.edit.changes[URI]);
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
    expect(apply(source, actions[0].edit.changes[URI])).toBe(
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

  it("says nothing about an expression rather than showing an empty popup", () => {
    // uf has no positional type query yet: `uf_check` exposes whole-file
    // diagnostics only. A `null` answer is an editor showing nothing; an empty
    // popup would be a confident "no type".
    const messages = session([didOpen("// @flow\nconst x = 1;\n"), hover(5, 1, 6), EXIT]);

    expect(answer(messages, 5).result).toBe(null);
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
