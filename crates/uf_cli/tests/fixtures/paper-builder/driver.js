// @noflow
//
// Plain JavaScript: the host runs this file directly, exactly as it runs
// `@uniflowed/vite/driver.js`. See that file, and `docs/architecture.md`, for
// the contract this implements.
//
//   <host> driver.js build --root <dir> [--mode <m>] [--out-dir <dir>]
//                          [--prerender everything|possible|nothing]
//
// Everything uf hands a builder arrives on the command line and in the
// environment; everything the builder says goes to stdout as one JSON event
// per line; and the process exits when its stdin closes so it cannot outlive
// the command that started it. None of that is Vite's, which is the claim this
// file exists to check.

import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";

function argument(name) {
  const at = process.argv.indexOf(name);
  return at === -1 ? null : process.argv[at + 1];
}

function emit(event, data) {
  process.stdout.write(`${JSON.stringify({ event, ...data })}\n`);
}

process.stdin.on("end", () => process.exit(0));
process.stdin.on("error", () => process.exit(0));
process.stdin.resume();

const command = process.argv[2];
const root = path.resolve(argument("--root") ?? process.cwd());

// The contract's answer for a command a builder has not implemented: say which
// ones it has, by name. A builder that answered "one moment, here is a
// directory" for a command nobody wrote would be the silent wrong answer uf
// refuses everywhere else.
if (command !== "build") {
  emit("error", {
    message: `the paper builder implements "build" and was asked for ${JSON.stringify(command)}`,
  });
  process.exit(2);
}

const outDir = path.resolve(root, argument("--out-dir") ?? "dist");
const prerender = argument("--prerender") ?? "possible";

emit("config-loaded", { file: path.join(root, "uf.config.js") });
emit("phase", { name: "paper" });

// The route table, from the filesystem. A page with parameters cannot be
// rendered without them, so it goes on the per-request list — the same
// three-way decision `@uniflowed/vite` makes, reached without a module graph.
const routes = pages(path.join(root, "app"), "/");
const prerendered = prerender === "nothing" ? [] : routes.filter((route) => !route.dynamic);
const perRequest = routes.filter((route) => prerender === "nothing" || route.dynamic);

emit("rendering", {
  prerender,
  prerendered: prerendered.length,
  perRequest: perRequest.map((route) => route.path),
});

if (prerender === "everything" && perRequest.length > 0) {
  emit("error", {
    message: `${perRequest.length} route(s) cannot be prerendered:\n${perRequest
      .map((route) => `  ${route.path}`)
      .join("\n")}`,
  });
  process.exit(1);
}

for (const route of prerendered) {
  const file =
    route.path === "/"
      ? path.join(outDir, "index.html")
      : path.join(outDir, route.path.slice(1), "index.html");
  mkdirSync(path.dirname(file), { recursive: true });
  // Paper: the route's own path, and a marker a test can look for. There is no
  // application in it, because rendering one would need the Flow transform
  // this builder deliberately does not have.
  const html = `<!doctype html>\n<html lang="en"><body><main data-paper-builder="${route.path}">${route.path}</main></body></html>\n`;
  writeFileSync(file, html);
  emit("page", {
    url: route.path,
    file: path.relative(root, file),
    status: 200,
    bytes: Buffer.byteLength(html),
  });
}

emit("done", { outDir: path.relative(root, outDir), pages: prerendered.length });
process.exit(0);

/** Every `$page.js` under `directory`, as `{ path, dynamic }`. */
function pages(directory, routePath) {
  let found = [];
  let entries;
  try {
    entries = readdirSync(directory, { withFileTypes: true });
  } catch {
    return found;
  }
  if (entries.some((entry) => entry.isFile() && entry.name === "$page.js")) {
    found.push({ path: routePath, dynamic: routePath.includes(":") });
  }
  for (const entry of entries) {
    if (!entry.isDirectory()) continue;
    const segment = entry.name.startsWith("[")
      ? `:${entry.name.replace(/^\[|\]$/g, "")}`
      : entry.name;
    const below = routePath === "/" ? `/${segment}` : `${routePath}/${segment}`;
    found = found.concat(pages(path.join(directory, entry.name), below));
  }
  return found;
}
