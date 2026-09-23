#!/usr/bin/env node
// The adapter-parity questions, asked of a deployment over the network.
//
//   node tools/ci/deployed-parity.mjs --reference <url> --deployed <url> [--label <name>]
//
// `crates/uf_cli/tests/vite.rs`'s `every_adapter_answers_exactly_what_the_node_adapter_answers`
// asks each adapter's artefact the same questions in-process, with the
// platform's contract standing in for the platform. That proves the artefacts
// agree with each other; it cannot prove that a Worker on Cloudflare or a
// function on Lambda answers the way the artefact did on Node. This asks the
// same questions of two URLs — `uf start` over the build as the reference, and
// the deployment the same build was turned into — and fails on the first
// answer that differs. `.github/workflows/deploy-parity.yml` is what runs it.
//
// The questions are `SERVED_APP_QUESTIONS` from that test, against the same
// fixture (`crates/uf_cli/tests/fixtures/served-app`), plus the two version-skew
// questions every front door has to answer alike: a request naming another
// build is refused with a `409`, and one naming the live build is not.
//
// What is compared is the status, the one header a question is about, and the
// body with the per-render anchor blanked (`<meta name="uf:render">` carries the
// instant and seed of each render, so two renders never match on it — the Rust
// test blanks it the same way). Nothing about timing, and nothing a platform
// adds on its own: a `cf-ray` header is not a disagreement about the
// application.
//
// It is also the conformance check the adapter contract offers a platform that
// implements it outside this repository: deploy the served-app fixture with
// your adapter, run `uf start` over the same build, and point this at both. See
// `docs/app/guide/deploy`.

import process from "node:process";

const argv = process.argv.slice(2);
const option = (name) => {
  const at = argv.indexOf(`--${name}`);
  return at === -1 ? null : (argv[at + 1] ?? null);
};

const reference = option("reference");
const deployed = option("deployed");
const label = option("label") ?? "deployed";
if (reference == null || deployed == null) {
  process.stderr.write(
    "usage: node tools/ci/deployed-parity.mjs --reference <url> --deployed <url> [--label <name>]\n",
  );
  process.exit(2);
}

/** The deployment id the reference's documents publish, for the skew questions. */
async function deploymentOf(base) {
  const response = await fetch(new URL("/posts/hello-world", base));
  const html = await response.text();
  return /<meta name="uf:deployment" content="([^"]+)">/.exec(html)?.[1] ?? null;
}

/**
 * Every question, as `[label, path, init, header]`. `header` is the one header
 * the answer is about, when the answer is a header; otherwise the body is.
 */
function questions(live) {
  const post = { method: "POST", body: JSON.stringify({ name: "uf" }) };
  const asked = [
    ["handler-get", "/api/health"],
    ["handler-post", "/api/health", post],
    ["rendered", "/posts/hello-world"],
    ["redirect", "/old/hello-world", undefined, "location"],
    ["missing", "/definitely-not-a-page/"],
    ["rule-redirect", "/moved/hello-world", undefined, "location"],
    ["rule-rewrite", "/articles/hello-world"],
    ["middleware-rewrite", "/shop/hello-world"],
    ["rewrite-payload", "/shop/hello-world/__uf.flight", undefined, "content-type"],
    ["rule-header", "/api/health", undefined, "cache-control"],
    ["rule-header-everywhere", "/posts/hello-world", undefined, "x-served-by"],
    ["prerendered", "/guide/"],
  ];
  if (live != null) {
    asked.push(
      [
        "skew-refused",
        "/api/health",
        { ...post, headers: { "uf-deployment": "0000000000000000" } },
        "uf-deployment",
      ],
      ["skew-live", "/api/health", { ...post, headers: { "uf-deployment": live } }],
    );
  }
  return asked;
}

/** One answer, as the line two doors are compared on. */
async function ask(base, [name, path, init, header]) {
  const response = await fetch(new URL(path, base), {
    ...init,
    headers: { accept: "text/html", ...init?.headers },
    redirect: "manual",
  });
  const body = (await response.text())
    .replace(/<meta name="uf:render" content="[^"]*"\s*\/?>/g, '<meta name="uf:render">')
    .replace(/\s+/g, " ")
    .trim();
  if (header == null) return `${name} ${response.status} ${body}`;
  let value = response.headers.get(header) ?? "-";
  // A platform may answer a redirect with the absolute URL of the same path;
  // the application said the path, and that is what is compared.
  if (header === "location" && /^https?:\/\//.test(value)) {
    const url = new URL(value);
    value = url.pathname + url.search;
  }
  return `${name} ${response.status} ${header}=${value}`;
}

const live = await deploymentOf(reference);
const failures = [];
for (const question of questions(live)) {
  const [expected, answered] = await Promise.all([
    ask(reference, question),
    ask(deployed, question),
  ]);
  if (expected === answered) {
    process.stdout.write(`  ok  ${question[0]}\n`);
  } else {
    failures.push(`${question[0]}\n    uf start: ${expected}\n    ${label}: ${answered}`);
    process.stdout.write(`  FAIL  ${question[0]}\n`);
  }
}
if (live == null) {
  // Not a failure of the deployment: a reference build that publishes no id is
  // a build from before skew protection, and the two questions are skipped by
  // name rather than silently.
  process.stdout.write(
    "  skip  skew-refused, skew-live: the reference publishes no deployment id\n",
  );
}
if (failures.length > 0) {
  process.stderr.write(
    `\n${label} answered differently from uf start:\n\n${failures.join("\n\n")}\n`,
  );
  process.exit(1);
}
process.stdout.write(`\n${label} answered every question the way uf start did.\n`);
