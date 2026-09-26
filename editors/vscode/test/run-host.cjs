const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { downloadAndUnzipVSCode, runTests } = require("@vscode/test-electron");
const { verifySyntax } = require("./syntax.cjs");

async function main() {
  const extension = path.resolve(__dirname, "..");
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-editor-host-"));
  const project = path.join(root, "project");
  fs.mkdirSync(path.join(project, "app"), { recursive: true });
  fs.mkdirSync(path.join(project, "generated"));
  fs.writeFileSync(path.join(project, "generated", "plain.js"), "const generated = 1;\n");
  fs.mkdirSync(path.join(project, ".vscode"));
  fs.writeFileSync(path.join(project, "uf.config.js"), "export default {};\n");
  fs.writeFileSync(path.join(project, "app", "useCounter.js"), "// @flow\nexport hook useCounter(initial: number): [number, () => void] { return [initial, () => {}]; }\n");
  fs.writeFileSync(path.join(project, "app", "Counter.js"), [
    "// @flow",
    "import { useCounter } from './useCounter.js';",
    "export component Counter(initial: number) {",
    "  const [count, increment] = useCounter(initial);",
    "  return <button onClick={increment}>{count}</button>;",
    "}",
    "",
  ].join("\n"));
  // Existing projects already have validation off. Opening them with the
  // upgrade must remove the built-in hover without replacing their settings.
  fs.writeFileSync(path.join(project, ".vscode", "settings.json"), JSON.stringify({
    "uf.server.path": path.resolve(process.env.UF_BINARY),
    "javascript.validate.enable": false,
    "files.associations": { "*.json": "jsonc", "**/generated/*.js": "javascript" },
  }));
  fs.writeFileSync(path.join(project, "plain.ts"), "const typed: number = 1;\n");
  const external = path.join(root, "external.js");
  fs.writeFileSync(external, "const plain = 1;\n");
  const plainFolder = path.join(root, "plain");
  fs.mkdirSync(plainFolder);
  const other = path.join(plainFolder, "external.js");
  fs.writeFileSync(other, "const plain = 1;\n");
  const workspace = path.join(root, "multi.code-workspace");
  fs.writeFileSync(workspace, JSON.stringify({ folders: [{ path: project }, { path: plainFolder }], settings: { "files.associations": { "**/generated/*.js": "javascript" } } }));
  const executable = process.env.VSCODE_EXECUTABLE_PATH ?? await downloadAndUnzipVSCode({ version: "stable" });
  const appRoot = process.platform === "darwin"
    ? path.resolve(executable, "..", "..", "Resources", "app")
    : path.resolve(executable, "..", "resources", "app");
  await verifySyntax(appRoot);
  for (const [name, target, plain] of [["folder", project, external], ["multi", workspace, other]]) {
    await runTests({
      vscodeExecutablePath: executable,
      extensionDevelopmentPath: extension,
      extensionTestsPath: path.join(__dirname, "host.js"),
      extensionTestsEnv: { UF_EDITOR_EXTERNAL_JS: plain },
      launchArgs: [target, "--disable-extensions", "--disable-workspace-trust", "--skip-welcome", "--skip-release-notes", "--no-sandbox", "--user-data-dir", path.join(root, name), "--extensions-dir", path.join(root, "extensions")],
    });
  }
  console.log(`Editor verification files: ${root}`);
}

main().catch((error) => { console.error(error); process.exitCode = 1; });
