// @noflow
//
// The questions the deploy matrix asks a running target, one per rendering
// mode (a row of `../matrix.json`).
//
// Every check is HTTP-level and asserts what a browser or a CDN would see:
// status, the headers the mode is about (cache headers included), the body, and
// — for the two modes that are about time — when each part of the body
// arrived. Every answer is computed from its question (an id echoed, a string
// reversed, an instant rendered), so a host's fallback page answering `200`
// cannot pass for the application.
//
// A check throws on the first thing that is wrong and returns a short list of
// observations otherwise; `../run.mjs` records both. It does not decide what
// the target *should* support — that is the cell's `status` in the matrix —
// with one exception: a cell may carry `expect`, which picks between two
// documented behaviours of the same mode (a streaming page on a target that
// buffers arrives whole, and that is asserted rather than excused).
//
// What is not here: the browser (`./browser.mjs`), and refusals, which are
// build-time and live in `../run.mjs` because a refused build never starts a
// host to ask.

import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import { setTimeout as sleep } from "node:timers/promises";

import { arrivalOf, request } from "./http.mjs";

/**
 * What a check is handed.
 *
 * @typedef {object} CheckContext
 * @property {string} base the target's origin
 * @property {string} target the matrix column, e.g. `edge`
 * @property {string | undefined} expect the cell's `expect`, when it has one
 * @property {string} buildDir the fixture copy that was built, for its manifest
 * @property {string} deployDir `.uf/deploy/<adapter>` inside it
 * @property {(() => Promise<void>) | null} restart stop the host and start it
 *   again on the same address and the same durable store; `null` where the
 *   harness cannot
 */

/** A unique suffix, so a check never meets an answer an earlier one cached. */
function unique(label) {
  return `${label}-${Date.now().toString(36)}${Math.random().toString(36).slice(2, 6)}`;
}

/** Fail with the answer attached, which is what makes a CI log readable. */
function describe(answer) {
  const headers = Array.from(answer.headers, ([name, value]) => `${name}: ${value}`).join("\n    ");
  return `status ${answer.status}\n    ${headers}\n  body: ${answer.body.slice(0, 800)}`;
}

function expectStatus(answer, status, what) {
  assert.equal(answer.status, status, `${what}: expected ${status}\n  ${describe(answer)}`);
}

function expectBody(answer, needle, what) {
  assert.ok(
    answer.body.includes(needle),
    `${what}: body lacks ${JSON.stringify(needle)}\n  ${describe(answer)}`,
  );
}

/** The instant a fixture page says it was rendered at, from `<label> rendered at <ms>`. */
function renderedAt(body, label) {
  const match = new RegExp(`${label} rendered at (\\d+)`).exec(body);
  return match == null ? null : Number(match[1]);
}

/** A `Location` as the path the application wrote, whatever origin a host prefixed. */
function locationPath(answer) {
  const value = answer.headers.get("location");
  if (value == null) return null;
  if (/^https?:\/\//.test(value)) {
    const url = new URL(value);
    return url.pathname + url.search;
  }
  return value;
}

/** The ids the build gave the fixture's server actions, by export name. */
function actionIds(buildDir) {
  const manifest = JSON.parse(
    readFileSync(path.join(buildDir, ".uf/build/meta/uf-rsc-manifest.json"), "utf8"),
  );
  const ids = new Map();
  for (const entry of manifest.serverActions ?? []) ids.set(entry.export, entry.id);
  return ids;
}

/** Decode the five entities React escapes in an attribute value. */
function unescapeAttribute(value) {
  return value
    .replaceAll("&quot;", '"')
    .replaceAll("&#x27;", "'")
    .replaceAll("&#39;", "'")
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

/**
 * The fields a form submits without JavaScript: every named `<input>` inside
 * the `<form id="…">`, with its value. What React and uf write for a form
 * bound to a server action (`$uf_ref_…`, `$ACTION_KEY`, …) is among them, and
 * posting them back is exactly what a browser does before hydration.
 */
function formFields(html, id) {
  const start = html.indexOf(`id="${id}"`);
  assert.ok(start !== -1, `the page has no form #${id}`);
  const open = html.lastIndexOf("<form", start);
  const close = html.indexOf("</form>", start);
  const form = html.slice(open, close);
  const fields = new URLSearchParams();
  for (const [input] of form.matchAll(/<input\b[^>]*>/g)) {
    const name = /\bname="([^"]*)"/.exec(input)?.[1];
    if (name == null) continue;
    const value = /\bvalue="([^"]*)"/.exec(input)?.[1] ?? "";
    fields.append(unescapeAttribute(name), unescapeAttribute(value));
  }
  const method = /\bmethod="([^"]*)"/i.exec(form)?.[1] ?? "GET";
  const enctype = /\benc[tT]ype="([^"]*)"/.exec(form)?.[1] ?? "application/x-www-form-urlencoded";
  const action = /\baction="([^"]*)"/.exec(form)?.[1] ?? null;
  return { fields, method: method.toUpperCase(), enctype, action };
}

/** `ssg`: prerendered pages, answered as the same file twice. */
async function ssg({ base }) {
  const home = await request(base, "/");
  expectStatus(home, 200, "GET /");
  expectBody(home, "matrix home", "GET /");
  const post = await request(base, "/posts/second");
  expectStatus(post, 200, "GET /posts/second");
  expectBody(post, "post: second", "GET /posts/second");
  // A prerendered document is a file: two answers are byte-identical, the
  // per-render anchor (`<meta name="uf:render">`, an instant and a seed)
  // included. A target that rendered it per request would differ there.
  const again = await request(base, "/posts/second");
  assert.equal(
    again.body,
    post.body,
    "GET /posts/second twice: the prerendered document changed between requests, so it was rendered rather than served",
  );
  return [`cache-control on a prerendered page: ${post.headers.get("cache-control") ?? "(none)"}`];
}

/** `ssr`: a page rendered per request, which reads a request header. */
async function ssr({ base }) {
  const id = unique("ssr");
  const answer = await request(base, `/ssr/${id}`, { headers: { "x-matrix-probe": "p1" } });
  expectStatus(answer, 200, `GET /ssr/${id}`);
  expectBody(answer, `ssr: ${id} probe: p1`, `GET /ssr/${id}`);
  const other = await request(base, `/ssr/${id}`, { headers: { "x-matrix-probe": "p2" } });
  expectBody(other, `ssr: ${id} probe: p2`, `GET /ssr/${id} with another probe`);
  return [];
}

/**
 * When `first` and then `second` arrived in one answer for `path`.
 *
 * Asked the way a browser asks — `accept-encoding: gzip, deflate, br` — because
 * a host that compresses may hold a small streamed chunk in its compressor,
 * and that is what a visitor would see. When the answer is not progressive the
 * same question is asked again with `identity`, and both are in the failure,
 * so a log says whether the application or the host's compression held it.
 */
async function arrival(base, path, first, second, headers = {}) {
  const ask = async (encoding) => {
    // One path per question: a page that suspends keeps its promise per id,
    // so asking the same id twice would be answered at once the second time.
    const asked = path();
    const answer = await request(base, asked, {
      headers: { ...headers, "accept-encoding": encoding },
    });
    expectStatus(answer, 200, `GET ${asked}`);
    const at = { first: arrivalOf(answer, first), second: arrivalOf(answer, second()) };
    assert.ok(
      at.first != null,
      `GET ${asked}: body lacks ${JSON.stringify(first)}\n  ${describe(answer)}`,
    );
    assert.ok(
      at.second != null,
      `GET ${asked}: body lacks ${JSON.stringify(second())}\n  ${describe(answer)}`,
    );
    return {
      gap: Math.round(at.second - at.first),
      first: Math.round(at.first),
      second: Math.round(at.second),
      reads: answer.chunks.length,
      encoding: answer.headers.get("content-encoding") ?? "none",
    };
  };
  const browser = await ask("gzip, deflate, br");
  const says = (label, seen) =>
    `${label}: ${JSON.stringify(first)} at ${seen.first} ms, the rest at ${seen.second} ms, ${seen.reads} reads, content-encoding ${seen.encoding}`;
  return {
    browser,
    says,
    /** The same question without compression, for a failure message. */
    identity: async () => says("with accept-encoding: identity", await ask("identity")),
  };
}

/**
 * Streaming SSR with Suspense.
 *
 * `expect: "whole"` is a target that buffers by design (a Lambda's payload is
 * one JSON value): the document must still be complete and correct, and the
 * fallback and the content must arrive together — asserting that keeps the
 * matrix honest about which targets stream.
 */
async function streaming({ base, expect }) {
  let id = "";
  const measured = await arrival(
    base,
    () => {
      id = unique("stream");
      return `/stream/${id}`;
    },
    "stream: waiting",
    () => `stream: ${id}`,
  );
  const { browser } = measured;
  if (expect === "whole") {
    assert.ok(
      browser.gap < 400,
      `a buffering target sent the fallback ${browser.gap} ms before the content; it streams after all — change the matrix`,
    );
    return [measured.says("arrived whole", browser)];
  }
  // The page waits 1200 ms; the fallback must be out well before that.
  if (browser.gap < 600) {
    assert.fail(
      `the body was not streamed. ${measured.says("as a browser asks", browser)}; ${await measured.identity()}`,
    );
  }
  return [measured.says("as a browser asks", browser)];
}

/** `ppr`: the static shell arrives first, the per-request hole about 1200 ms later. */
async function ppr({ base }) {
  const measured = await arrival(
    base,
    () => "/ppr",
    "ppr shell",
    () => "ppr hole for ada",
    {
      cookie: "who=ada",
    },
  );
  if (measured.browser.gap < 600) {
    assert.fail(
      `the shell was not sent before the hole. ${measured.says("as a browser asks", measured.browser)}; ${await measured.identity()}`,
    );
  }
  return [measured.says("as a browser asks", measured.browser)];
}

/** The instant the build rendered `page` at, from the regeneration copy it wrote. */
function builtInstant(deployDir, page, label) {
  const staticRoot = ["static", "."]
    .map((dir) => path.join(deployDir, dir, "__uf/regenerate", page, "index.html"))
    .find((file) => {
      try {
        readFileSync(file);
        return true;
      } catch {
        return false;
      }
    });
  assert.ok(staticRoot != null, `the build wrote no regeneration document for /${page}`);
  const instant = renderedAt(readFileSync(staticRoot, "utf8"), label);
  assert.ok(instant != null, `the build's /${page} document names no instant`);
  return instant;
}

/**
 * `isr`, time-based: the build's document first, a regenerated one once the
 * two-second lifetime has passed, and — after a restart — still the
 * regenerated one, read back from the target's durable store.
 */
async function isrTime({ base, deployDir, restart }) {
  const built = builtInstant(deployDir, "isr", "isr");
  const first = await request(base, "/isr");
  expectStatus(first, 200, "GET /isr");
  const cache = first.headers.get("x-uf-cache");
  assert.ok(
    ["HIT", "STALE"].includes(cache ?? ""),
    `GET /isr: x-uf-cache is ${cache}, expected HIT or STALE for the build's document\n  ${describe(first)}`,
  );
  assert.equal(
    renderedAt(first.body, "isr"),
    built,
    "GET /isr: the first answer is not the build's document",
  );
  let regenerated = null;
  for (let attempt = 0; attempt < 30 && regenerated == null; attempt++) {
    await sleep(1000);
    const answer = await request(base, "/isr");
    expectStatus(answer, 200, "GET /isr");
    const instant = renderedAt(answer.body, "isr");
    if (instant != null && instant !== built) regenerated = instant;
  }
  assert.ok(
    regenerated != null,
    "GET /isr still answered the build's document 30 s past its 2 s lifetime",
  );
  assert.ok(
    regenerated > built,
    `GET /isr regenerated to an instant before the build's (${regenerated} < ${built})`,
  );
  const notes = [`build ${built}, regenerated ${regenerated}, first x-uf-cache ${cache}`];
  if (restart != null) {
    await restart();
    const after = await request(base, "/isr");
    expectStatus(after, 200, "GET /isr after a restart");
    const instant = renderedAt(after.body, "isr");
    assert.ok(
      instant != null && instant !== built,
      "after a restart GET /isr answered the build's document: the regenerated page was not kept in the durable store",
    );
    notes.push(`after a restart: ${instant} (not the build's)`);
  }
  return notes;
}

/**
 * `isr`, on demand: an hour-long page stays the same until
 * `revalidateTag("isr-tag")`, after which the next answer is a fresh render
 * (`x-uf-cache: MISS`) — and a restarted host does not go back to the build.
 */
async function isrOnDemand({ base, deployDir, restart }) {
  const built = builtInstant(deployDir, "tagged", "tagged");
  const before = await request(base, "/tagged");
  expectStatus(before, 200, "GET /tagged");
  assert.equal(
    renderedAt(before.body, "tagged"),
    built,
    "GET /tagged: the first answer is not the build's document",
  );
  const expired = await request(base, "/api/revalidate?tag=isr-tag", { method: "POST" });
  expectStatus(expired, 200, "POST /api/revalidate?tag=isr-tag");
  let fresh = null;
  let cache = null;
  for (let attempt = 0; attempt < 10 && fresh == null; attempt++) {
    const answer = await request(base, "/tagged");
    expectStatus(answer, 200, "GET /tagged after the invalidation");
    const instant = renderedAt(answer.body, "tagged");
    if (instant != null && instant !== built) {
      fresh = instant;
      cache = answer.headers.get("x-uf-cache");
    } else {
      await sleep(500);
    }
  }
  assert.ok(fresh != null, "GET /tagged still answered the build's document after revalidateTag");
  const notes = [`build ${built}, after revalidateTag ${fresh} (x-uf-cache ${cache})`];
  if (restart != null) {
    await restart();
    const after = await request(base, "/tagged");
    const instant = renderedAt(after.body, "tagged");
    assert.ok(
      instant != null && instant !== built,
      "after a restart GET /tagged answered the build's document again: the invalidation was not kept in the durable store",
    );
    notes.push(`after a restart: ${instant} (not the build's)`);
  }
  return notes;
}

/** RSC navigation: the Flight payload a `<Link>` fetches, prerendered and rendered. */
async function rscPayload({ base }) {
  const prerendered = await request(base, "/posts/first/__uf.flight");
  expectStatus(prerendered, 200, "GET /posts/first/__uf.flight");
  assert.match(
    prerendered.headers.get("content-type") ?? "",
    /text\/x-component/,
    `GET /posts/first/__uf.flight is not a Flight payload\n  ${describe(prerendered)}`,
  );
  expectBody(prerendered, "post: first", "GET /posts/first/__uf.flight");
  const id = unique("payload");
  const rendered = await request(base, `/ssr/${id}/__uf.flight`);
  expectStatus(rendered, 200, `GET /ssr/${id}/__uf.flight`);
  assert.match(
    rendered.headers.get("content-type") ?? "",
    /text\/x-component/,
    `GET /ssr/${id}/__uf.flight is not a Flight payload`,
  );
  expectBody(rendered, `ssr: ${id}`, `GET /ssr/${id}/__uf.flight`);
  return [];
}

/**
 * A server action called the way a hydrated client reference calls it, and the
 * same call from another origin refused.
 */
async function actionJson({ base, buildDir }) {
  const id = actionIds(buildDir).get("add");
  assert.ok(id != null, "the build's manifest names no `add` action");
  const call = (origin) =>
    request(base, "/actions", {
      method: "POST",
      headers: {
        origin,
        "content-type": "application/json",
        "uf-action": id,
        cookie: "visitor=ada",
      },
      body: JSON.stringify({ args: [41] }),
    });
  const answer = await call(new URL(base).origin);
  expectStatus(answer, 200, "POST /actions (add)");
  expectBody(answer, '"total":42', "POST /actions (add)");
  expectBody(answer, '"visitor":"ada"', "POST /actions (add)");
  const refused = await call("http://evil.example");
  expectStatus(refused, 403, "POST /actions (add) from another origin");
  return [];
}

/**
 * The pre-hydration path: the form's own fields, posted as a browser without
 * JavaScript would, answered with the page rendered again holding the
 * action's state.
 */
async function actionForm({ base }) {
  const page = await request(base, "/actions");
  expectStatus(page, 200, "GET /actions");
  const form = formFields(page.body, "note-form");
  assert.equal(form.method, "POST", "the action form is not a POST before hydration");
  assert.ok(
    Array.from(form.fields.keys()).some((name) => name.startsWith("$")),
    `the action form carries no action fields: ${form.fields.toString()}`,
  );
  form.fields.set("note", "matrix");
  const answer = await request(base, form.action ?? "/actions", {
    method: "POST",
    headers: {
      origin: new URL(base).origin,
      "content-type": "application/x-www-form-urlencoded",
    },
    body: form.fields.toString(),
  });
  expectStatus(answer, 200, "POST /actions (the form, before hydration)");
  assert.match(
    answer.headers.get("content-type") ?? "",
    /text\/html/,
    "the form post was not answered with a page",
  );
  expectBody(answer, "saved: xirtam", "POST /actions (the form)");
  return [];
}

/** Route handlers: both methods echo, and `app.router.headers` marks them `no-store`. */
async function routeHandlers({ base }) {
  const got = await request(base, "/api/echo?q=matrix");
  expectStatus(got, 200, "GET /api/echo");
  expectBody(got, '"query":"matrix"', "GET /api/echo");
  assert.equal(
    got.headers.get("cache-control"),
    "no-store",
    "GET /api/echo: the `cache-control` rule was not applied",
  );
  const posted = await request(base, "/api/echo", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name: "matrix" }),
  });
  expectStatus(posted, 200, "POST /api/echo");
  expectBody(posted, '"echoed":"matrix"', "POST /api/echo");
  return [];
}

/** Cookies both ways: the request's `Cookie` reaches the handler, two `Set-Cookie`s stay two. */
async function cookies({ base }) {
  const answer = await request(base, "/api/cookies", { headers: { cookie: "c=3" } });
  expectStatus(answer, 200, "GET /api/cookies");
  expectBody(answer, '"received":"c=3"', "GET /api/cookies");
  assert.equal(
    answer.setCookies.length,
    2,
    `GET /api/cookies: expected two Set-Cookie headers, got ${JSON.stringify(answer.setCookies)}`,
  );
  assert.ok(
    answer.setCookies[1].includes("Expires=Wed, 21 Oct 2037"),
    "the second cookie lost its Expires attribute",
  );
  return [];
}

/** A middleware rewrite: `/gated/<id>` answered by `/ssr/<id>`. */
async function middleware({ base }) {
  const id = unique("gated");
  const answer = await request(base, `/gated/${id}`, { headers: { "x-matrix-probe": "mw" } });
  expectStatus(answer, 200, `GET /gated/${id}`);
  expectBody(answer, `ssr: ${id} probe: mw`, `GET /gated/${id}`);
  return [];
}

/** `app.router` redirects, rewrites and headers, and a loader's redirect. */
async function routerRules({ base }) {
  const moved = await request(base, "/moved/first");
  expectStatus(moved, 308, "GET /moved/first");
  assert.equal(locationPath(moved), "/posts/first", "GET /moved/first: wrong Location");
  const rewritten = await request(base, "/articles/first");
  expectStatus(rewritten, 200, "GET /articles/first");
  expectBody(rewritten, "post: first", "GET /articles/first");
  const loader = await request(base, "/old/first");
  expectStatus(loader, 307, "GET /old/first");
  assert.equal(
    locationPath(loader),
    "/posts/first",
    "GET /old/first: the loader's Location was dropped",
  );
  for (const [what, answer] of [
    ["a prerendered page", await request(base, "/")],
    ["a rendered page", await request(base, `/ssr/${unique("rules")}`)],
    ["a route handler", await request(base, "/api/echo")],
  ]) {
    assert.equal(
      answer.headers.get("x-matrix"),
      "deploy-matrix",
      `the x-matrix header rule was not applied to ${what}\n  ${describe(answer)}`,
    );
  }
  return [];
}

/** The project's 404, for a path no route matches and for `notFound()` in a render. */
async function notFound({ base, expect }) {
  const missing = await request(base, "/no/such/page");
  expectStatus(missing, 404, "GET /no/such/page");
  expectBody(missing, "matrix has no such page", "GET /no/such/page");
  if (expect !== "static") {
    const thrown = await request(base, "/ssr/none");
    expectStatus(thrown, 404, "GET /ssr/none");
    expectBody(thrown, "matrix has no such page", "GET /ssr/none");
  }
  return [];
}

/** A page that throws at request time: `$error.js`, inside the layout, with a `500`. */
async function errorBoundary({ base }) {
  const answer = await request(base, `/boom/${unique("boom")}`);
  expectStatus(answer, 500, "GET /boom/…");
  expectBody(answer, "boom boundary: thrown", "GET /boom/…");
  expectBody(answer, 'id="hydration"', "GET /boom/… (the layout around the boundary)");
  assert.ok(!answer.body.includes("throws on purpose"), "the error's message reached the browser");
  return [];
}

/** The client bundle: a hashed script the document names, served as a script. */
async function staticAssets({ base }) {
  const home = await request(base, "/");
  const script = /<script type="module" src="([^"]+)"/.exec(home.body)?.[1];
  assert.ok(script != null, "GET /: the document names no module script");
  const answer = await request(base, script);
  expectStatus(answer, 200, `GET ${script}`);
  assert.match(
    answer.headers.get("content-type") ?? "",
    /javascript/,
    `GET ${script} was not answered as a script`,
  );
  const missing = await request(base, "/assets/no-such-chunk-0000.js");
  assert.notEqual(missing.status, 200, "a script that does not exist answered 200");
  return [`${script}: cache-control ${answer.headers.get("cache-control") ?? "(none)"}`];
}

/**
 * Every HTTP check, by the mode id `../matrix.json` uses. `hydration` is the
 * browser's and `images` has no check yet (its cells are `planned`).
 */
export const CHECKS = {
  ssg,
  ssr,
  streaming,
  ppr,
  "isr-time": isrTime,
  "isr-on-demand": isrOnDemand,
  "rsc-payload": rscPayload,
  "action-json": actionJson,
  "action-form": actionForm,
  "route-handlers": routeHandlers,
  cookies,
  middleware,
  "router-rules": routerRules,
  "not-found": notFound,
  "error-boundary": errorBoundary,
  "static-assets": staticAssets,
};
