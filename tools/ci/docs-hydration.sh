#!/usr/bin/env sh
set -eu

script_dir="$(CDPATH= cd "$(dirname "$0")" && pwd)"
repo_root="$(CDPATH= cd "$script_dir/../.." && pwd)"
site_dir="$repo_root/docs/dist/docs"
browser="${UF_BROWSER:-}"

usage() {
  echo "usage: tools/ci/docs-hydration.sh [--site DIR] [--browser PATH]" >&2
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --site)
      if [ "$#" -lt 2 ]; then
        usage
        exit 2
      fi
      site_dir="$2"
      shift 2
      ;;
    --browser)
      if [ "$#" -lt 2 ]; then
        usage
        exit 2
      fi
      browser="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      usage
      exit 2
      ;;
  esac
done

if [ ! -d "$site_dir" ]; then
  echo "docs-hydration: $site_dir does not exist; run docs:build first" >&2
  exit 1
fi

site_dir="$(CDPATH= cd "$site_dir" && pwd)"

if [ -z "$browser" ]; then
  for candidate in google-chrome-stable google-chrome chrome chromium; do
    if command -v "$candidate" >/dev/null 2>&1; then
      browser="$(command -v "$candidate")"
      break
    fi
  done
fi

if [ -z "$browser" ]; then
  echo "docs-hydration: no Chromium-family browser found" >&2
  exit 1
fi

UF_DOCS_HYDRATION_SITE="$site_dir" \
UF_DOCS_HYDRATION_BROWSER="$browser" \
node <<'NODE'
"use strict";

const fs = require("node:fs");
const http = require("node:http");
const os = require("node:os");
const path = require("node:path");
const { spawn } = require("node:child_process");

const site = process.env.UF_DOCS_HYDRATION_SITE;
const browser = process.env.UF_DOCS_HYDRATION_BROWSER;
const profile = fs.mkdtempSync(path.join(os.tmpdir(), "uf-docs-hydration-"));

const HYDRATION_ERROR =
  /Minified React error #418|Hydration failed|hydration|server rendered HTML|did not match/i;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

function timeout(label, ms) {
  return new Promise((_, reject) => {
    setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
  });
}

function withTimeout(promise, label, ms) {
  return Promise.race([promise, timeout(label, ms)]);
}

function mimeType(file) {
  switch (path.extname(file)) {
    case ".css":
      return "text/css; charset=utf-8";
    case ".gif":
      return "image/gif";
    case ".html":
      return "text/html; charset=utf-8";
    case ".ico":
      return "image/x-icon";
    case ".jpg":
    case ".jpeg":
      return "image/jpeg";
    case ".js":
    case ".mjs":
      return "text/javascript; charset=utf-8";
    case ".json":
      return "application/json; charset=utf-8";
    case ".png":
      return "image/png";
    case ".svg":
      return "image/svg+xml; charset=utf-8";
    case ".txt":
      return "text/plain; charset=utf-8";
    case ".webp":
      return "image/webp";
    case ".xml":
      return "application/xml; charset=utf-8";
    default:
      return "application/octet-stream";
  }
}

function safeJoin(root, pathname) {
  const resolved = path.resolve(root, `.${pathname}`);
  if (resolved !== root && !resolved.startsWith(`${root}${path.sep}`)) {
    return null;
  }
  return resolved;
}

function staticFileFor(requestUrl) {
  const url = new URL(requestUrl, "http://docs.local");
  const pathname = decodeURIComponent(url.pathname);
  const candidates = [];

  if (pathname.endsWith("/")) {
    candidates.push(safeJoin(site, `${pathname}index.html`));
  } else {
    candidates.push(safeJoin(site, pathname));
    candidates.push(safeJoin(site, `${pathname}.html`));
    candidates.push(safeJoin(site, `${pathname}/index.html`));
  }

  for (const candidate of candidates) {
    if (candidate && fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
      return candidate;
    }
  }
  return null;
}

function collectPages(dir, prefix = "") {
  const pages = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    if (entry.name.startsWith(".")) {
      continue;
    }
    const relative = path.join(prefix, entry.name);
    const absolute = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      pages.push(...collectPages(absolute, relative));
    } else if (entry.isFile() && entry.name.endsWith(".html")) {
      let route = `/${relative.split(path.sep).join("/")}`;
      if (route.endsWith("/index.html")) {
        route = route.slice(0, -"index.html".length);
      } else {
        route = route.slice(0, -".html".length);
      }
      pages.push(route || "/");
    }
  }
  return [...new Set(pages)].sort((left, right) => left.localeCompare(right));
}

function startServer() {
  const server = http.createServer((request, response) => {
    const file = staticFileFor(request.url || "/");
    if (!file) {
      response.writeHead(404, { "content-type": "text/plain; charset=utf-8" });
      response.end("not found");
      return;
    }

    response.writeHead(200, {
      "cache-control": "no-store",
      "content-type": mimeType(file),
    });
    fs.createReadStream(file).pipe(response);
  });

  return new Promise((resolve, reject) => {
    server.on("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      resolve({ server, origin: `http://127.0.0.1:${address.port}` });
    });
  });
}

function openWebSocket(url) {
  return new Promise((resolve, reject) => {
    const socket = new WebSocket(url);
    socket.addEventListener("open", () => resolve(socket), { once: true });
    socket.addEventListener("error", () => reject(new Error(`failed to open ${url}`)), {
      once: true,
    });
  });
}

class Cdp {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Set();

    socket.addEventListener("message", (event) => {
      const message = JSON.parse(String(event.data));
      if (message.id && this.pending.has(message.id)) {
        const { resolve, reject } = this.pending.get(message.id);
        this.pending.delete(message.id);
        if (message.error) {
          reject(new Error(message.error.message || "CDP command failed"));
        } else {
          resolve(message.result || {});
        }
        return;
      }

      for (const listener of this.listeners) {
        listener(message);
      }
    });
  }

  send(method, params = {}, sessionId) {
    const id = this.nextId++;
    const payload = sessionId ? { id, method, params, sessionId } : { id, method, params };
    const promise = new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    this.socket.send(JSON.stringify(payload));
    return promise;
  }

  on(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  waitFor(method, sessionId) {
    return new Promise((resolve) => {
      const off = this.on((message) => {
        if (message.method === method && message.sessionId === sessionId) {
          off();
          resolve(message.params || {});
        }
      });
    });
  }
}

function argText(arg) {
  if (Object.prototype.hasOwnProperty.call(arg, "value")) {
    return String(arg.value);
  }
  return arg.description || arg.unserializableValue || arg.type || "";
}

function consoleText(args) {
  return args.map(argText).filter(Boolean).join(" ");
}

async function startChrome() {
  const args = [
    "--headless=new",
    "--disable-background-networking",
    "--disable-dev-shm-usage",
    "--disable-gpu",
    "--disable-setuid-sandbox",
    "--no-default-browser-check",
    "--no-first-run",
    "--no-sandbox",
    "--remote-debugging-port=0",
    `--user-data-dir=${profile}`,
    "about:blank",
  ];
  const chrome = spawn(browser, args, {
    stdio: ["ignore", "ignore", "pipe"],
  });

  let stderr = "";
  const wsUrl = await withTimeout(
    new Promise((resolve, reject) => {
      chrome.once("error", reject);
      chrome.stderr.on("data", (chunk) => {
        stderr += chunk.toString();
        const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
        if (match) {
          resolve(match[1]);
        }
      });
      chrome.once("exit", (code, signal) => {
        reject(new Error(`browser exited before CDP was ready: ${code || signal}\n${stderr}`));
      });
    }),
    "browser startup",
    15000,
  );

  return { chrome, wsUrl };
}

async function stopChrome(chrome) {
  if (chrome.exitCode !== null || chrome.signalCode !== null) {
    return;
  }
  chrome.kill("SIGTERM");
  await Promise.race([
    new Promise((resolve) => chrome.once("exit", resolve)),
    sleep(2000).then(() => chrome.kill("SIGKILL")),
  ]);
}

async function checkPage(cdp, sessionId, origin, route, width) {
  const messages = [];
  const off = cdp.on((message) => {
    if (message.sessionId !== sessionId) {
      return;
    }

    if (message.method === "Runtime.consoleAPICalled") {
      const text = consoleText(message.params.args || []);
      if (HYDRATION_ERROR.test(text)) {
        messages.push({
          kind: "console",
          type: message.params.type || "log",
          text,
        });
      }
    } else if (message.method === "Runtime.exceptionThrown") {
      const details = message.params.exceptionDetails || {};
      const text = [
        details.text,
        details.exception && argText(details.exception),
        details.exception && details.exception.description,
      ]
        .filter(Boolean)
        .join(" ");
      if (HYDRATION_ERROR.test(text)) {
        messages.push({ kind: "exception", type: "error", text });
      }
    } else if (message.method === "Log.entryAdded") {
      const entry = message.params.entry || {};
      const text = entry.text || "";
      if (HYDRATION_ERROR.test(text)) {
        messages.push({ kind: "log", type: entry.level || "error", text });
      }
    }
  });

  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    {
      width,
      height: 900,
      deviceScaleFactor: 1,
      mobile: width < 600,
      screenWidth: width,
      screenHeight: 900,
    },
    sessionId,
  );

  const loaded = cdp.waitFor("Page.loadEventFired", sessionId);
  await cdp.send("Page.navigate", { url: `${origin}${route}` }, sessionId);
  await withTimeout(loaded, `${route} ${width}px load`, 15000);
  await sleep(1500);
  off();

  return messages.map((message) => ({ route, width, ...message }));
}

async function main() {
  const pages = collectPages(site);
  if (pages.length === 0) {
    throw new Error(`no HTML pages found in ${site}`);
  }

  const { server, origin } = await startServer();
  let chrome;
  try {
    const started = await startChrome();
    chrome = started.chrome;
    const socket = await openWebSocket(started.wsUrl);
    const cdp = new Cdp(socket);
    const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
    const { sessionId } = await cdp.send("Target.attachToTarget", {
      targetId,
      flatten: true,
    });
    await cdp.send("Runtime.enable", {}, sessionId);
    await cdp.send("Log.enable", {}, sessionId);
    await cdp.send("Page.enable", {}, sessionId);

    const findings = [];
    for (const route of pages) {
      for (const width of [464, 1440]) {
        findings.push(...(await checkPage(cdp, sessionId, origin, route, width)));
      }
    }

    await cdp.send("Target.closeTarget", { targetId });
    socket.close();

    if (findings.length > 0) {
      console.error("docs-hydration: hydration errors detected");
      for (const finding of findings) {
        console.error(
          `docs-hydration: ${finding.width}px ${finding.route}: ` +
            `${finding.kind}/${finding.type}: ${finding.text}`,
        );
      }
      process.exitCode = 1;
      return;
    }

    console.log(`docs-hydration: checked ${pages.length} pages at 464px and 1440px`);
  } finally {
    server.close();
    if (chrome) {
      await stopChrome(chrome);
    }
    fs.rmSync(profile, { recursive: true, force: true });
  }
}

main().catch((error) => {
  console.error(`docs-hydration: ${error.message}`);
  process.exit(1);
});
NODE
