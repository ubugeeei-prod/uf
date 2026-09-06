// @flow
//
// The on-disk transform cache, and what it is allowed to reuse.
//
// `packages/host/internal/node-hooks.js` keeps every module it compiles under
// `.uf/cache/transform/`, and the key has to name the compiler as well as the
// source. For a long time it named only the source, so a rebuilt `uf` went on
// serving what the previous one had produced — silently, because a stale entry
// is still a perfectly valid module. These are the tests for that.
//
// A checkout has one `uf` and this needs two builds of it, so the binary here
// is a stand-in. That is a smaller substitution than it sounds: the loader's
// entire contract with `uf` is the newline-delimited JSON in
// `packages/host/transform.js` — one request per line, one reply per line, in
// order — and a program that honours it is a compiler as far as the loader is
// concerned. Everything around it is real: the real `register.js`, the real
// hooks, a real Node process, a real cache on a real disk.

import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

/** The loader under test, reached as a path so no resolution is involved. */
const REGISTER: string = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "../../packages/host/register.js",
);

/** The token the stand-in compiler substitutes, so its output names its build. */
const MARK = "__BUILD__";

// Two moments a pair of rebuilds could sit between, a day apart so that no
// filesystem's timestamp granularity can confuse them for each other.
const FIRST = Date.UTC(2026, 0, 1);
const SECOND = Date.UTC(2026, 0, 2);

/** What one run of the project said. */
type Result = { status: number | null, stdout: string, stderr: string };

/**
 * A stand-in for `uf transform`, as a program the loader can actually run.
 *
 * It replaces the mark with `marker` and passes every other byte through, so
 * two of these are two compilers that disagree about one source — which is the
 * whole situation being tested. Each request it answers is announced on
 * stderr, which is how a test below tells a compile from a cache hit; the
 * loader inherits the compiler's stderr, so those lines arrive with the run's.
 */
const compiler = (marker: string): string => `#!${process.execPath}
const MARKER = ${JSON.stringify(marker)};
let rest = "";
process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  rest += chunk;
  let at = rest.indexOf("\\n");
  while (at !== -1) {
    const request = JSON.parse(rest.slice(0, at));
    rest = rest.slice(at + 1);
    process.stderr.write("compiled " + request.id + "\\n");
    const code = request.code.split(${JSON.stringify(MARK)}).join(MARKER);
    process.stdout.write(JSON.stringify({ code }) + "\\n");
    at = rest.indexOf("\\n");
  }
});
`;

/** A project of two modules: one that carries the mark, one that prints it. */
const project = (): string => {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-transform-cache-")));
  fs.writeFileSync(path.join(root, "thing.js"), `// @flow\nexport const compiledBy = "${MARK}";\n`);
  fs.writeFileSync(
    path.join(root, "main.js"),
    'import { compiledBy } from "./thing.js";\nprocess.stdout.write(compiledBy);\n',
  );
  return root;
};

/**
 * The same two modules, with an entry that rebuilds `uf` before it imports the
 * other one.
 *
 * The rebuild has to land *between* the loader being installed and the first
 * module the compiler is actually asked about, and that is a window a test
 * cannot open from outside the process. So the entry opens it: on the warm run
 * it comes out of the cache without the compiler being started at all, and
 * what it then does is replace the binary and reach for a module that is not
 * cached under the new one.
 *
 * The import is dynamic because it has to happen *after* the swap above; the
 * `await` in front of it is a top-level one, which uf reads as a module's since
 * ubugeeei-prod/uf#204.
 */
const projectThatRebuilds = (): string => {
  const root = project();
  fs.writeFileSync(
    path.join(root, "main.js"),
    'import fs from "node:fs";\n' +
      "const next = process.env.UF_NEXT_BUILD;\n" +
      "if (next != null && fs.existsSync(next)) {\n" +
      "  fs.writeFileSync(process.env.UF_BINARY, fs.readFileSync(next));\n" +
      "  fs.chmodSync(process.env.UF_BINARY, 0o755);\n" +
      "  const when = new Date(Number(process.env.UF_NEXT_WHEN));\n" +
      "  fs.utimesSync(process.env.UF_BINARY, when, when);\n" +
      "}\n" +
      'const thing = await import("./thing.js");\n' +
      "process.stdout.write(thing.compiledBy);\n",
  );
  return root;
};

/** Run `body` against a fresh project, and take the project away afterwards. */
const inAProject = (body: (root: string) => void, make?: () => string): void => {
  const root = (make ?? project)();
  try {
    body(root);
  } finally {
    fs.rmSync(root, { recursive: true, force: true });
  }
};

/**
 * Put a build of the stand-in compiler at `<root>/uf`, over whatever is there.
 *
 * A rebuild is a new file at the same path, and what identifies it is its size
 * and its modification time. The time is set rather than left to the clock:
 * two builds in one test can land in the same millisecond, and a filesystem
 * that records whole seconds would hand both the same one. `cargo build` is
 * never that quick and needs no such help.
 */
const buildUf = (root: string, marker: string, when: number): string => {
  const binary = path.join(root, "uf");
  fs.writeFileSync(binary, compiler(marker));
  fs.chmodSync(binary, 0o755);
  fs.utimesSync(binary, new Date(when), new Date(when));
  return binary;
};

/**
 * Run the project's entry module under the real loader.
 *
 * `env` is laid over this process's environment, and a name mapped to
 * `undefined` is removed from it — which is how the last test gets a host with
 * no `UF_BINARY` at all, the way one started by hand has none.
 */
const run = (root: string, env: { [string]: string | void }): Result => {
  const environment = { ...process.env, UF_PROJECT_ROOT: root, ...env };
  for (const name of Object.keys(environment)) {
    if (environment[name] == null) delete environment[name];
  }
  const result = spawnSync(process.execPath, ["--import", REGISTER, path.join(root, "main.js")], {
    cwd: root,
    env: environment,
    encoding: "utf8",
  });
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
};

/** Everything the cache is holding for this project. */
const cached = (root: string): Array<string> => {
  try {
    return fs.readdirSync(path.join(root, ".uf", "cache", "transform"));
  } catch {
    return [];
  }
};

describe("the transform cache", () => {
  it("serves a second run from disk rather than compiling again", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      const cold = run(root, { UF_BINARY: binary });
      const warm = run(root, { UF_BINARY: binary });

      expect(cold.stdout).toBe("first-build");
      expect(cold.stderr).toContain("compiled ");
      expect(warm.stdout).toBe("first-build");
      // The compiler was asked nothing at all the second time. This is what
      // the cache is for, and every test below has to leave it true.
      expect(warm.stderr).not.toContain("compiled ");
    });
  });

  it("compiles again when uf itself was rebuilt", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      expect(run(root, { UF_BINARY: binary }).stdout).toBe("first-build");

      // The same source, the same path, a different build: what
      // `cargo build --release --bin uf` leaves behind after an edit to
      // `crates/uf_transform`.
      buildUf(root, "second-build", SECOND);
      const after = run(root, { UF_BINARY: binary });

      // This said "first-build" while the key knew the source and nothing
      // about who had compiled it, and a module the new compiler had never
      // seen was handed to the run as if it had.
      expect(after.stdout).toBe("second-build");
      expect(after.stderr).toContain("compiled ");
    });
  });

  it("still has a build's entries when that build comes back", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      run(root, { UF_BINARY: binary });
      buildUf(root, "second-build", SECOND);
      run(root, { UF_BINARY: binary });

      // Back to the first build, byte for byte and to the same second. The key
      // identifies a build rather than counting them, so a bisect that walks
      // back over a compiler change is not paying for a cold cache each time.
      buildUf(root, "first-build", FIRST);
      const again = run(root, { UF_BINARY: binary });

      expect(again.stdout).toBe("first-build");
      expect(again.stderr).not.toContain("compiled ");
      // Two modules, two builds, four entries: they sit beside each other
      // rather than one generation evicting the last.
      expect(cached(root)).toHaveLength(4);
    });
  });

  it("does not serve what it cached when it cannot tell which uf compiled it", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      expect(run(root, { UF_BINARY: binary }).stdout).toBe("first-build");

      // No compiler now, and a cache full of what it produced. Failing is the
      // only honest answer: those entries were written by something this
      // checkout can no longer point at, and serving them would let a run
      // succeed on a machine with nothing to compile with.
      fs.rmSync(binary);
      const orphaned = run(root, { UF_BINARY: binary });

      expect(orphaned.status).not.toBe(0);
      expect(orphaned.stdout).not.toContain("first-build");
    });
  });

  it("does not serve a build's entries to the build that replaced it mid-run", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      const next = path.join(root, "uf.next");
      const env = { UF_BINARY: binary, UF_NEXT_BUILD: next, UF_NEXT_WHEN: String(SECOND) };

      // Cold, then warm: the second run serves both modules from disk and
      // starts no compiler, which is what leaves the window open below.
      expect(run(root, env).stdout).toBe("first-build");
      expect(run(root, env).stderr).not.toContain("compiled ");

      // Now the entry — itself a cache hit — replaces `uf` and only then
      // reaches for the module the compiler would be asked about. The binary
      // was one build when the hooks were installed and is another by the time
      // anything is compiled, and the run that reads the key once at
      // installation serves the old build's output while the new build is what
      // would run: the defect this key exists to remove, with a smaller
      // window.
      fs.writeFileSync(next, compiler("second-build"));
      const across = run(root, env);

      expect(across.stdout).toBe("second-build");
      expect(across.stderr).toContain("compiled ");
    }, projectThatRebuilds);
  });

  it("does not serve what it cached through a binary that can no longer run", () => {
    inAProject((root) => {
      const binary = buildUf(root, "first-build", FIRST);
      expect(run(root, { UF_BINARY: binary }).stdout).toBe("first-build");

      // Size and modification time do not move when a file loses its execute
      // bit, so a key built from those alone was the same key as before: the
      // warm run went on serving and a cold one could not start `uf` at all.
      // Whether the command worked then depended on how warm the cache was,
      // which is the class of answer this key exists to remove.
      fs.chmodSync(binary, 0o644);
      const unusable = run(root, { UF_BINARY: binary });

      expect(unusable.status).not.toBe(0);
      expect(unusable.stdout).not.toContain("first-build");
    });
  });

  it("identifies the binary a host started by hand finds on PATH", () => {
    inAProject((root) => {
      buildUf(root, "first-build", FIRST);
      // `node --import @uniflowed/host/register app.js` is a documented way to
      // run a Flow project and sets no `UF_BINARY`; `ufBinary()` falls back to
      // `uf` on PATH. The identity has to follow it there. An entry point that
      // quietly cached under no compiler at all would be the same defect,
      // kept alive for everyone `uf` did not start.
      const byHand = { UF_BINARY: undefined, PATH: root };
      expect(run(root, byHand).stdout).toBe("first-build");
      expect(run(root, byHand).stderr).not.toContain("compiled ");

      buildUf(root, "second-build", SECOND);
      expect(run(root, byHand).stdout).toBe("second-build");
    });
  });
});
