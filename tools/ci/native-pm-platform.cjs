"use flow";

const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { execFileSync } = require("node:child_process");

const archive = process.argv[2];
if (!archive) throw new Error("Expected a release archive path");
const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "uf-native-platform-"));
const runtime = path.join(temporary, "runtime");
const project = path.join(temporary, "project");
fs.mkdirSync(runtime);
fs.mkdirSync(project);
const environment = {
  ...process.env,
  CI         : "1",
  NO_COLOR   : "1",
  UF_PM_STORE: path.join(temporary, "store"),
  UF_STORE   : path.join(temporary, "tools"),
  UF_ROOTS   : path.join(temporary, "roots"),
};

function run(argv /*: $ReadOnlyArray<string> */) /*: string */ {
  return execFileSync(argv[0], argv.slice(1), {
    cwd     : project,
    env     : environment,
    encoding: "utf8",
    timeout : 180000,
  });
}

try {
  const tar =
    process.platform === "win32"
      ? path.join(process.env.SystemRoot || "C:\\Windows", "System32", "tar.exe")
      : "tar";
  execFileSync(tar, ["-xzf", path.resolve(archive), "-C", runtime], {
    timeout: 120000,
  });
  const uf = path.join(runtime, "bin", process.platform === "win32" ? "uf.exe" : "uf");
  fs.writeFileSync(
    path.join(project, "package.json"),
    JSON.stringify({
      name        : "native-platform-fixture",
      private     : true,
      type        : "module",
      dependencies: { vite: "8.3.1", "@uniflowed/test": "0.13.0" },
    }),
  );
  fs.writeFileSync(
    path.join(project, "uf.config.js"),
    "export default { pm: { packageManager: 'uf' } };\n",
  );
  run([uf, "install"]);
  fs.writeFileSync(
    path.join(project, "native.test.js"),
    '"use flow";\nimport { expect, it } from "@uniflowed/test";\nit("loads the transitive host", () => { const value: number = 2; expect(value).toBe(2); });\n',
  );
  const tests = JSON.parse(run([uf, "test", "--json"]));
  if (tests.passed !== 1 || tests.failed !== 0) {
    throw new Error(`Native test runner failed: ${JSON.stringify(tests)}`);
  }
  const lock = fs.readFileSync(path.join(project, "uf.lock"));
  run([uf, "install", "--frozen-lockfile"]);
  if (!lock.equals(fs.readFileSync(path.join(project, "uf.lock")))) {
    throw new Error("Frozen installation changed the lockfile");
  }
  const vite = run([process.execPath, "node_modules/vite/bin/vite.js", "--version"]);
  if (!vite.startsWith("vite/8.3.1")) throw new Error(`Unexpected Vite result: ${vite}`);
  fs.writeFileSync(
    path.join(project, "index.html"),
    '<script type="module" src="/main.js"></script>',
  );
  fs.writeFileSync(
    path.join(project, "main.js"),
    'document.body.textContent = "native-platform-ready";\n',
  );
  run([process.execPath, "node_modules/vite/bin/vite.js", "build"]);
  const assets = path.join(project, "dist", "assets");
  if (
    !fs
      .readdirSync(assets)
      .some((name) =>
        fs.readFileSync(path.join(assets, name), "utf8").includes("native-platform-ready"),
      )
  ) {
    throw new Error("Vite did not build the application with its native platform binding");
  }
  console.log(
    `${run([uf, "--version"]).trim()} / ${process.platform}-${process.arch}: native Vite install, frozen reuse and platform build passed`,
  );
} finally {
  fs.rmSync(temporary, { recursive: true, force: true, maxRetries: 3, retryDelay: 100 });
}
