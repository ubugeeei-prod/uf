// @flow
//
// `uf mcp`, driven the way an agent drives it.
//
// The sibling of `lsp.test.js`, and deliberately so: both are JSON-RPC over
// stdio and they are *not* the same wire. LSP frames with `Content-Length`
// headers; MCP's stdio transport is one JSON message per line. A server that
// confused the two would look right in every unit test and would not talk to
// any client, so the framing is asserted here, against the real binary, rather
// than assumed.
//
// The binary is the one running this suite: `uf test` puts its own path in
// `UF_BINARY`, which is what makes this a test about this checkout rather than
// about whatever `uf` is on PATH.
//
// What is checked here and not in `crates/uf_cli/src/commands/mcp/tests.rs` is
// everything that only a subprocess can show: the framing, the fact that
// nothing else writes to stdout, and that a tool's answer survives the trip.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "@uniflowed/test";

const UF: string = (() => {
  const binary = process.env.UF_BINARY;
  if (binary == null || binary === "") {
    throw new Error(
      "UF_BINARY is not set: this test drives `uf mcp`, and `uf test` is what names the binary",
    );
  }
  return binary;
})();

type Message = { [string]: mixed };

type Content = { type: string, text: string };
type Tool = { name: string, description: string, inputSchema: { type: string, ... } };
type Result = {
  protocolVersion?: string,
  capabilities?: { tools?: { ... } },
  serverInfo?: { name: string, version: string },
  tools?: Array<Tool>,
  content?: Array<Content>,
  isError?: boolean,
};
type Wire = {
  jsonrpc: string,
  id?: number,
  result?: Result,
  error?: { code: number, message: string },
};

/**
 * Run one conversation against `uf mcp` and parse everything it said.
 *
 * One message per line going in, one reply per line coming back — and the
 * parse below is the assertion that it *is* one per line: a stray banner, a
 * log line, or a `Content-Length` header would make `JSON.parse` throw.
 */
const session = (messages: Array<Message>, cwd: string): Array<Wire> => {
  const run = spawnSync(UF, ["mcp"], {
    input: messages.map((message) => `${JSON.stringify(message)}\n`).join(""),
    cwd,
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (run.status !== 0) {
    throw new Error(`uf mcp exited with ${String(run.status)}: ${String(run.stderr)}`);
  }
  return run.stdout
    .split("\n")
    .filter((line) => line.trim() !== "")
    .map((line) => JSON.parse(line));
};

const request = (id: number, method: string, params?: Message): Message => ({
  jsonrpc: "2.0",
  id,
  method,
  ...(params == null ? {} : { params }),
});

const INITIALIZE: Message = request(1, "initialize", {
  protocolVersion: "2024-11-05",
  capabilities: {},
  clientInfo: { name: "uf-test", version: "0" },
});

const answered = (messages: Array<Wire>, id: number): Result => {
  const found = messages.find((message) => message.id === id);
  if (found == null) {
    throw new Error(`no answer for id ${id} in ${JSON.stringify(messages)}`);
  }
  if (found.result == null) {
    throw new Error(`id ${id} answered with an error: ${JSON.stringify(found.error)}`);
  }
  return found.result;
};

/** The text blocks of a `tools/call` answer. */
const blocks = (messages: Array<Wire>, id: number): Array<Content> => {
  const content = answered(messages, id).content;
  if (content == null) {
    throw new Error(`id ${id} was not a tool result`);
  }
  return content;
};

const callTool = (id: number, name: string, args: Message = {}): Message =>
  request(id, "tools/call", { name, arguments: args });

/** A project uf can load, whose one source file does not parse. */
const brokenProject = (): string => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-mcp-"));
  fs.writeFileSync(path.join(root, "uf.config.js"), "export default {};\n");
  fs.writeFileSync(path.join(root, "package.json"), '{"name":"broken","private":true}\n');
  fs.mkdirSync(path.join(root, "src"));
  fs.writeFileSync(path.join(root, "src", "bad.js"), "// @flow\nexport function f( {\n");
  return root;
};

describe("uf mcp", () => {
  it("speaks newline-delimited JSON, not LSP's framing", () => {
    const root = brokenProject();
    const run = spawnSync(UF, ["mcp"], {
      input: `${JSON.stringify(INITIALIZE)}\n`,
      cwd: root,
      encoding: "utf8",
    });

    expect(run.status).toBe(0);
    // The whole of stdout is one line of JSON: no header, no banner, nothing
    // a client would have to skip past.
    expect(run.stdout).not.toContain("Content-Length");
    expect(run.stdout.trimEnd().split("\n")).toHaveLength(1);
    expect(JSON.parse(run.stdout).result.serverInfo.name).toBe("uf");
  });

  it("answers initialize with the version it speaks and the tools capability", () => {
    const out = session([INITIALIZE], brokenProject());
    const result = answered(out, 1);

    expect(result.protocolVersion).toBe("2024-11-05");
    expect(result.capabilities?.tools).toBeDefined();
    expect(result.serverInfo?.name).toBe("uf");
  });

  it("does not answer a notification", () => {
    const out = session(
      [INITIALIZE, { jsonrpc: "2.0", method: "notifications/initialized" }, request(2, "ping")],
      brokenProject(),
    );

    // Two requests, two replies. A third would desynchronise a client that
    // counts them.
    expect(out).toHaveLength(2);
    expect(out.map((message) => message.id)).toEqual([1, 2]);
  });

  it("lists the commands as tools, and names the ones that write", () => {
    const out = session([INITIALIZE, request(2, "tools/list")], brokenProject());
    const tools = answered(out, 2).tools ?? [];

    expect(tools.map((tool) => tool.name)).toEqual([
      "uf_check",
      "uf_lint",
      "uf_info",
      "uf_routes",
      "uf_explain",
      "uf_test",
      "uf_fmt_write",
      "uf_lint_fix",
    ]);
    for (const tool of tools) {
      expect(tool.inputSchema.type).toBe("object");
      const writes = tool.name.endsWith("_write") || tool.name.endsWith("_fix");
      expect(tool.description.includes("WRITES")).toBe(writes);
    }
  });

  // The two halves of the bug the first real MCP client found: a command that
  // renders for a reader returned nothing when it was run in JSON mode, and a
  // command that failed had the reason appended to its JSON report.
  it("returns the text of a command that renders for a reader", () => {
    const out = session([INITIALIZE, callTool(2, "uf_info")], brokenProject());
    const content = blocks(out, 2);

    expect(answered(out, 2).isError).toBe(false);
    expect(content).toHaveLength(1);
    expect(content[0].text.length).toBeGreaterThan(0);
  });

  it("keeps a failing command's report parseable, in its own block", () => {
    const out = session([INITIALIZE, callTool(2, "uf_lint")], brokenProject());
    const content = blocks(out, 2);

    expect(answered(out, 2).isError).toBe(true);
    expect(content).toHaveLength(2);

    // The report, whole, exactly as `uf lint --json` writes it.
    const report = JSON.parse(content[0].text);
    expect(report.command).toBe("uf lint");
    expect(report.errors).toBeGreaterThan(0);
    // And the reason beside it, where it cannot corrupt the report.
    expect(content[1].text).toContain("uf lint");
  });

  it("answers an unknown tool as a tool error, not a protocol error", () => {
    const out = session([INITIALIZE, callTool(2, "uf_rm_rf")], brokenProject());

    expect(out.find((message) => message.id === 2)?.error).toBeUndefined();
    expect(answered(out, 2).isError).toBe(true);
    expect(blocks(out, 2)[0].text).toContain("uf_rm_rf");
  });

  it("answers an unknown method as a protocol error", () => {
    const out = session([INITIALIZE, request(2, "resources/list")], brokenProject());
    const found = out.find((message) => message.id === 2);

    expect(found?.error?.code).toBe(-32601);
    expect(found?.result).toBeUndefined();
  });

  // `uf lsp` reads `load_config(".")` and so ignores the `--cwd` it was given,
  // which is why every editor README under `editors/` has to talk about the
  // working directory. `uf mcp` takes the resolved directory as an argument,
  // and this is the test that keeps it that way: an agent naming the project
  // on the command line is the ordinary case, not the exception.
  it("honours --cwd, unlike uf lsp", () => {
    const root = brokenProject();
    const elsewhere = fs.mkdtempSync(path.join(os.tmpdir(), "uf-mcp-elsewhere-"));
    const run = spawnSync(UF, ["--cwd", root, "mcp"], {
      input: `${JSON.stringify(INITIALIZE)}\n${JSON.stringify(callTool(2, "uf_lint"))}\n`,
      cwd: elsewhere,
      encoding: "utf8",
      maxBuffer: 32 * 1024 * 1024,
    });
    const out = run.stdout
      .split("\n")
      .filter((line) => line.trim() !== "")
      .map((line) => JSON.parse(line));

    // The broken file is in `root`, and `elsewhere` is empty: a server that
    // read its own working directory would have found nothing to report.
    const report = JSON.parse(blocks(out, 2)[0].text);
    expect(report.errors).toBeGreaterThan(0);
    expect(report.diagnostics[0].path).toBe("src/bad.js");
  });

  it("survives a line that is not JSON", () => {
    const root = brokenProject();
    const run = spawnSync(UF, ["mcp"], {
      input: `{ not json\n${JSON.stringify(request(2, "ping"))}\n`,
      cwd: root,
      encoding: "utf8",
    });
    const out = run.stdout
      .split("\n")
      .filter((line) => line.trim() !== "")
      .map((line) => JSON.parse(line));

    expect(out).toHaveLength(2);
    expect(out[0].error?.code).toBe(-32700);
    // And the request behind the bad line was still answered, which is the
    // point: one client's bad frame must not lose everything after it.
    expect(out[1].id).toBe(2);
  });
});
