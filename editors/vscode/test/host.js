const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vscode = require("vscode");

async function waitFor(run, label) {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    const result = await run();
    if (result) return result;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out: ${label}`);
}

async function run() {
  const extension = vscode.extensions.getExtension("uniflowed.uf");
  assert.ok(extension, "development extension registered");
  await extension.activate();
  const root = vscode.workspace.workspaceFolders[0].uri.fsPath;
  const uri = vscode.Uri.file(path.join(root, "app", "Counter.js"));
  const document = await vscode.workspace.openTextDocument(uri);
  await vscode.window.showTextDocument(document);
  await waitFor(() => document.languageId === "flow", "project upgraded to Flow language mode");
  const position = new vscode.Position(3, 10);
  const hovers = await waitFor(async () => {
    const results = await vscode.commands.executeCommand("vscode.executeHoverProvider", uri, position);
    return results?.length ? results : null;
  }, "uf hover");
  const rendered = hovers.flatMap((hover) => hover.contents.map((content) => typeof content === "string" ? content : content.value)).join("\n");
  assert.equal(hovers.length, 1, `one provider: ${rendered}`);
  assert.match(rendered, /```flow\nconst count: number\n```/);
  assert.doesNotMatch(rendered, /\bany\b|loading/i);
  const completions = await vscode.commands.executeCommand("vscode.executeCompletionItemProvider", uri, new vscode.Position(4, document.lineAt(4).text.indexOf("{count}") + 1));
  assert.ok(completions.items.some((item) => item.label === "count"), "uf completion stays registered in Flow mode");
  const edits = await vscode.commands.executeCommand("vscode.executeFormatDocumentProvider", uri, { tabSize: 2, insertSpaces: true });
  assert.ok(edits?.length, "uf formatter stays registered in Flow mode");
  const config = vscode.workspace.getConfiguration("files", uri);
  const associations = config.get("associations");
  assert.equal(associations["*.js"], undefined, "automatic mode selection does not change window-wide associations");
  const settings = JSON.parse(fs.readFileSync(path.join(root, ".vscode/settings.json"), "utf8"));
  assert.equal(settings["files.associations"]["*.json"], "jsonc", "existing association preserved on disk even when window-scoped settings are ignored in multi-root mode");
  const ts = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(root, "plain.ts")));
  assert.equal(ts.languageId, "typescript", "TypeScript keeps its language id");
  const generated = await vscode.workspace.openTextDocument(vscode.Uri.file(path.join(root, "generated", "plain.js")));
  await vscode.window.showTextDocument(generated);
  // Let onDidOpen's async language selection finish before checking.
  await new Promise((resolve) => setTimeout(resolve, 300));
  assert.equal(generated.languageId, "javascript", "explicit path association keeps its language id");
  const external = await vscode.workspace.openTextDocument(vscode.Uri.file(process.env.UF_EDITOR_EXTERNAL_JS));
  assert.equal(external.languageId, "javascript", "JavaScript outside the uf folder stays JavaScript");
  console.log("Real VS Code: one typed Flow hover, completion, formatter, preserved settings, and JS/TS isolation passed");
  // Save the generated settings so the outer runner can inspect the result.
  await vscode.workspace.saveAll();
}

module.exports = { run };
