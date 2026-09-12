// @flow
//
// The process `uf test --browser` fans work out to.
//
// `./worker.js` runs a test file. This runs a *browser* that runs a test file,
// and from `uf`'s side the two are the same thing: a process started the same
// way, given a request per line on stdin, answering with an event per line on
// stdout, bounded by the same wall clock and replaced the same way when it
// stops answering. `crates/uf_test/src/host.rs` does not branch on which of
// the two it is talking to, and that is the whole design — the browser is
// another host, not a second runner.
//
//   uf ──stdin──▶ this process ──HTTP──▶ a page in a real browser
//      ◀─stdout──               ◀─HTTP──
//
// # Why there is a process in the middle at all
//
// A page cannot read a pipe. Somebody has to hold the browser's process
// handle, serve it modules, and turn what it says into what `uf` reads, and
// that somebody has to be a JavaScript host because resolving a bare specifier
// and transforming Flow are already solved there. So the driver is Node, the
// browser is the host under test, and the split is named in `docs/hosts.md`
// rather than left for a reader to infer from a stack trace.
//
// # What this depends on, exactly
//
// One browser binary, already installed, found or named by
// `crates/uf_test/src/browser.rs` and handed over in `UF_BROWSER`. Not
// downloaded, not vendored, not version-pinned, and not a driver library:
// there is no Playwright, no Puppeteer and no DevTools protocol here. The
// browser is started with a URL and a scratch profile and is otherwise left
// alone; everything uf needs to know comes back over HTTP from the page's own
// code.
//
// That is a deliberate ceiling as much as a deliberate floor. **uf cannot
// drive this browser** — it cannot click, navigate, screenshot, throttle a
// network or read a console it was not handed. Those are the other half of
// `docs/roadmap.md`'s "Playwright-compatible browser automation" and they want
// a protocol; this half only wants a page, and buying the protocol to get the
// page would have made a test runner own a browser automation library.
//
// # How this fails
//
// Loudly, by name, in three places. The browser is not there — refused in
// Rust, before a process is started. The browser starts and never asks for
// work — refused here, with whatever it wrote to stderr, because a browser
// that will not open a page is not a suite that failed. The browser goes away
// mid-run — the stream ends, and `uf` reports a host that died, which is the
// path Node workers already take.

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

import { inSourceTests, sharedService } from "@uniflowed/host/transform";

import { create } from "./internal/browser/server.js";

/** What `uf` sends for one file. */
type Request = {|
  readonly file: string,
  readonly filter?: string | null,
  readonly timeoutMs?: number,
  readonly generation?: number,
|};

/**
 * How long a browser is given to open the page before the run is refused.
 *
 * Generous against a cold Chromium with a cold profile, which is about seven
 * seconds on a laptop and much worse in a busy container, and deliberately
 * *shorter than `uf_test`'s browser file budget* (`BROWSER_FILE_TIMEOUT`, one
 * hundred twenty seconds).
 * That order is the whole point: `uf` is already holding a stopwatch on the
 * first file while this is happening, and whichever of the two fires first is
 * what the report says. A browser that will not start should be reported as a
 * browser that will not start, with its own output attached — not as a file
 * that timed out, which is a sentence about tests that were never reached.
 */
const START_TIMEOUT_MS = 90_000;

/**
 * The switches a headless run needs, and what each is for.
 *
 * Kept short on purpose. Every flag here is a decision about the browser the
 * tests are measured in, and a long list is a long list of ways this page
 * differs from the application's — which is the difference browser mode exists
 * to remove.
 */
function browserArguments(profile: string, url: string): Array<string> {
  return [
    // The modern headless, which is the same renderer as a windowed browser.
    // The old one was a separate binary with its own layout quirks, and a
    // layout answer from it would not have been an answer about Chrome.
    "--headless=new",
    // What makes the browser die when this process does, and the only reason
    // this mode names a debugging switch at all.
    //
    // `uf` kills a worker that misses its deadline, and a killed process runs
    // no exit handler — so without this, a timed-out file would leave a browser
    // running with nobody to stop it, and a suite that leaks one browser per
    // timeout eventually cannot run. Chrome treats the pipe as its lifeline:
    // when the file descriptors close, for any reason, it shuts itself down.
    //
    // Not a protocol. uf opens the two descriptors and never writes a byte to
    // them; the page reports over HTTP, as everything else here does. The pipe
    // is also why this is not `--remote-debugging-port`, which would open a
    // port on the machine that anything could speak to — a pipe is held by this
    // process alone.
    //
    // The tab closing is not enough on its own, which is what this replaces: a
    // page that calls `window.close()` frees the renderers and leaves the
    // browser process running (measured on Chrome 148).
    "--remote-debugging-pipe",
    // A scratch profile per run: no extensions, no saved state, no policy from
    // the developer's own browser leaking into what the suite measures.
    `--user-data-dir=${profile}`,
    "--no-first-run",
    "--no-default-browser-check",
    // Nothing here paints, and a GPU process is one more thing to fail in a
    // container.
    "--disable-gpu",
    // `/dev/shm` is 64 MB in most containers, which is where a renderer dies
    // in CI and nowhere else.
    "--disable-dev-shm-usage",
    "--disable-background-timer-throttling",
    "--disable-renderer-backgrounding",
    url,
  ];
}

/**
 * Write one event, exactly as `./worker.js` does.
 *
 * The raw stream, taken before anything in this process could have replaced
 * it. Nothing here captures `console`: the driver runs no test code, and the
 * printing that has to be captured happens in the page.
 */
const emit = (line: string) => {
  process.stdout.write(line);
};

function write(event: { readonly [string]: mixed }): void {
  emit(`${JSON.stringify(event)}\n`);
}

/** Report that the run cannot happen, against the file that asked for it. */
function refuse(message: string, generation: number): void {
  write({ event: "file", status: "run-failed", message, generation });
}

async function main(): Promise<void> {
  const root = process.env.UF_PROJECT_ROOT ?? process.cwd();
  const browser = process.env.UF_BROWSER;
  const service = sharedService(root);

  const server = await create({
    root,
    transform: async (id, code) => {
      // The same three the Node loader passes (`packages/host/internal/
      // node-hooks.js`), so a module means the same thing on both hosts.
      // `inSourceTests` is the one that would be easy to forget and expensive
      // to: without it `import.meta.uf.test` compiles to `void 0`, every
      // in-source block is shaken out, and a browser run of a project that
      // writes them reports fewer tests than the Node run of the same files —
      // silently, and with a green summary.
      const result = await service.transform(id, code, {
        development: true,
        sourceMap: true,
        inSourceTests: inSourceTests(),
      });
      if (result == null) return null;
      // The map is appended rather than served beside the module: a browser
      // reads `sourceMappingURL` from the end of the file, and a data URL is
      // one fewer route and one fewer round trip. What it buys is a stack
      // frame in a failure that names the line the author wrote — the same
      // thing `--enable-source-maps` buys a Node worker.
      const map = result.map;
      if (map == null) return result.code;
      const encoded = Buffer.from(typeof map === "string" ? map : JSON.stringify(map)).toString(
        "base64",
      );
      return `${result.code}\n//# sourceMappingURL=data:application/json;charset=utf-8;base64,${encoded}\n`;
    },
    warn: (message) => {
      process.stderr.write(`[uf] ${message}\n`);
    },
  });

  server.onEvent((event) => {
    write(event);
  });

  if (browser == null || browser === "") {
    // Belt and braces: `uf` refuses before starting this process, so reaching
    // here means somebody ran the driver by hand.
    refuse(
      "`@uniflowed/test/browser-worker.js` was started with no UF_BROWSER. It is not a command to run directly; `uf test --browser` finds a browser and names it here.",
      0,
    );
    await server.close();
    return;
  }

  const profile = mkdtempSync(path.join(tmpdir(), "uf-browser-"));
  // Five descriptors, because `--remote-debugging-pipe` reads 3 and writes 4.
  // They are opened and never used; see the flag's note for what that buys.
  const child = spawn(browser, browserArguments(profile, server.url), {
    stdio: ["ignore", "pipe", "pipe", "pipe", "pipe"],
  });
  // Kept, not printed. A Chromium writes a dozen lines about GPU probing and
  // Vulkan on a healthy start, and forwarding those to a passing run's report
  // would be noise; they are the whole of the evidence when it does not start,
  // and that is when they are shown.
  let noise = "";
  const keep = (chunk: mixed) => {
    if (noise.length < 8 * 1024) noise += String(chunk);
  };
  child.stdout?.on("data", keep);
  child.stderr?.on("data", keep);

  let started = false;
  const startFailure = await new Promise<string | null>((resolve) => {
    const deadline = setTimeout(() => {
      resolve(`the browser did not open the page within ${String(START_TIMEOUT_MS / 1000)}s`);
    }, START_TIMEOUT_MS);
    // Polled rather than pushed: the page's first request *is* the signal, and
    // the server already knows when it arrives. A `Page.loadEventFired` would
    // be the protocol this mode does not speak.
    const poll: IntervalID = setInterval(() => {
      if (!server.connected()) return;
      clearInterval(poll);
      clearTimeout(deadline);
      started = true;
      resolve(null);
    }, 25);
    child.on("exit", (code: mixed) => {
      if (started) return;
      clearInterval(poll);
      clearTimeout(deadline);
      resolve(`the browser exited (${String(code)}) before opening the page`);
    });
    child.on("error", (error: Error) => {
      clearInterval(poll);
      clearTimeout(deadline);
      resolve(`the browser could not be started: ${error.message}`);
    });
  });

  const stop = () => {
    child.kill("SIGKILL");
    try {
      rmSync(profile, { recursive: true, force: true });
    } catch {
      // A scratch profile that outlives the run is litter in the system
      // temporary directory, not a reason to fail a suite.
    }
  };

  if (startFailure != null) {
    stop();
    await server.close();
    // The first file's request is what `uf` is waiting on, so the refusal is
    // reported against it — every subsequent file gets the same message from a
    // fresh driver, which is the same shape as a worker that cannot spawn.
    const trailer = noise.trim() === "" ? "" : `\n${noise.trim()}`;
    refuse(`\`${browser}\`: ${startFailure}.${trailer}`, 0);
    // The stream ends here, which `uf` reads as a host that died. That is
    // exactly what happened, and it is what makes `uf` start a fresh driver
    // for the next file rather than send it to a browser that is not there.
    return;
  }

  createInterface({ input: process.stdin }).on("line", (line) => {
    if (line.trim() === "") return;
    let request: Request;
    try {
      request = JSON.parse(line);
    } catch (error) {
      write({
        event: "file",
        status: "run-failed",
        message: `malformed request: ${String(error)}`,
        generation: 0,
      });
      return;
    }
    // Handed straight over. `uf` sends one file at a time and waits for its
    // `file` event, and the page runs one file per load, so there is no queue
    // to keep here — the two ends already agree on the shape.
    server.offer({
      file: request.file,
      filter: request.filter ?? null,
      timeoutMs: request.timeoutMs ?? 5000,
      generation: request.generation ?? 0,
    });
  });

  process.stdin.on("close", () => {
    void server.close().then(() => {
      stop();
      service.close();
      process.exit(0);
    });
  });
}

// A driver that throws has no file to blame, so it says so against the request
// `uf` is waiting on and lets the stream end. Silence here would be a run that
// waits for a deadline.
main().catch((error: mixed) => {
  const reason = error instanceof Error ? error.message : String(error);
  refuse(`\`uf test --browser\` could not start: ${reason}`, 0);
  process.exit(1);
});
