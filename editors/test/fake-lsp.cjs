// @flow
// A language server that answers just enough for an editor to attach it, and
// tells the editor where it was started.
//
// The editor integrations under `editors/` are configuration: which command,
// in which directory, for which files, and which other servers to keep away
// from a uf project. Those decisions are what `editors/test/*` checks, in the
// editors themselves (headless Neovim, batch Emacs), and none of them needs
// the real `uf lsp` — `tests/library/lsp.test.js` covers that. So this stands
// in for it: `initialize` answers with `serverInfo.name` set to the name on
// its command line and one diagnostic per opened document whose message is
// the server's working directory, which is what `uf lsp` reads its
// configuration from.

"use strict";

const name = process.argv[2] ?? "fake";
let buffer = Buffer.alloc(0);

function send(message /*: mixed */) {
  const body = Buffer.from(JSON.stringify(message), "utf8");
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function handle(message /*: { id?: number, method?: string, params?: { textDocument?: { uri: string } } } */) {
  switch (message.method) {
    case "initialize":
      send({
        jsonrpc: "2.0",
        id: message.id,
        result: { capabilities: { textDocumentSync: 1 }, serverInfo: { name, version: "0.0.0" } },
      });
      return;
    case "textDocument/didOpen":
      send({
        jsonrpc: "2.0",
        method: "textDocument/publishDiagnostics",
        params: {
          uri: message.params?.textDocument?.uri,
          diagnostics: [
            {
              range: { start: { line: 0, character: 0 }, end: { line: 0, character: 1 } },
              severity: 2,
              source: name,
              message: `cwd=${process.cwd()}`,
            },
          ],
        },
      });
      return;
    case "shutdown":
      send({ jsonrpc: "2.0", id: message.id, result: null });
      return;
    case "exit":
      process.exit(0);
      return;
    default:
      if (message.id != null && message.method != null) {
        send({ jsonrpc: "2.0", id: message.id, result: null });
      }
  }
}

process.stdin.on("data", (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  for (;;) {
    const header = buffer.indexOf("\r\n\r\n");
    if (header < 0) {
      return;
    }
    const match = /Content-Length: (\d+)/i.exec(buffer.subarray(0, header).toString("utf8"));
    const length = match == null ? 0 : Number(match[1]);
    if (buffer.length < header + 4 + length) {
      return;
    }
    const body = buffer.subarray(header + 4, header + 4 + length).toString("utf8");
    buffer = buffer.subarray(header + 4 + length);
    handle(JSON.parse(body));
  }
});
