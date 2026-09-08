const SETUP_URL = "https://setup.uniflowed.dev";

// The content security policy the documentation site is served under.
//
// The site where uf argues about security sent no `content-security-policy` at
// all, which is ubugeeei-prod/uf#598. What is below is **derived from what the
// site actually loads** rather than copied from a template, and
// `tools/ci/docs-csp.sh` re-derives it from `docs/dist/docs` on every run of
// `uf run docs:verify`: it drives this handler, reads the header off the
// response, and fails when a built page loads something the policy forbids or
// when the header stops being sent at all. A policy nothing checks is a
// comment, and this document's own rule 6 says so.
//
// Every directive, and why it is what it is:
//
//   * `default-src 'self'` — the site is one origin. Every script, stylesheet,
//     image and font it references is a path, and there is no analytics, no
//     CDN, no font host and no comment widget. The `https://` URLs in the HTML
//     are `<a href>` targets and a `@context` in the JSON-LD, and a navigation
//     is not a fetch.
//   * `script-src` — `'self'` for `/assets/client-*.js`, plus a hash for each
//     of the four inline scripts a build actually emits. Two are uf's: the
//     theme bootstrap, which has to be inline and synchronous because a stored
//     dark preference applied after the first frame is a white flash, and the
//     `application/ld+json` block, which browsers do subject to `script-src`.
//     Two are React's streaming runtime — the `$RT` timing stub and the
//     `$RB`/`$RV` reveal function that flushes a Suspense boundary — and those
//     are the reason the checker recomputes rather than trusts: a React upgrade
//     changes their bytes, and a hash that has gone stale means Suspense
//     content that never appears. It fails in CI with the line to paste rather
//     than in a browser nobody is watching.
//   * `style-src 'self' 'unsafe-inline'` — Shiki colours every token of every
//     code sample with a `style` attribute, and one page has 1,564 of them.
//     Hashing 9,602 attribute values is not a policy anybody can maintain, and
//     `style-src-attr` alone would fall back to `style-src` on a browser that
//     does not implement it and take the colour away. A style attribute cannot
//     run a script; this is the directive where the site pays for its syntax
//     highlighting, and it is the only `'unsafe-inline'` here.
//   * `object-src 'none'`, `base-uri 'none'`, `frame-src 'none'`,
//     `frame-ancestors 'none'`, `form-action 'none'` — the site has no plugin,
//     no `<base>`, no `<iframe>` and no `<form>`, so each of these costs
//     nothing and closes a class. `base-uri` is the one that matters most: it
//     is how an injected `<base>` turns every relative script URL into
//     somebody else's.
//   * `upgrade-insecure-requests` — the site is served over HTTPS and every
//     reference in it is a path, so this is belt to the braces of the
//     `default-src`.
//
// There is no `'unsafe-eval'` and no `'strict-dynamic'`, and the site is
// prerendered — so there is no nonce either. A nonce is per response and every
// document here is a file on a CDN; claiming one would mean a worker rewriting
// HTML, which is a different program from this one.
const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  [
    "script-src 'self'",
    // The theme bootstrap, from `docs/app/_design/theme.js`.
    "'sha256-GfUNcJf52UpaQd6LU2T1KTtYHa1CrhXX4vgwslYyF14='",
    // The site's `application/ld+json`, from `docs/app/_uf.layout.js`.
    "'sha256-O/S3orjEiDkT0iCx3O669yJorYrgNFq2eYh8Es05qlM='",
    // React's streaming runtime: the timing stub, and the reveal function.
    "'sha256-7mu4H06fwDCjmnxxr/xNHyuQC6pLTHr4M2E4jXw5WZs='",
    "'sha256-QAlSewaQLi/NPCznjAZSyvQ72heD0VdxmNDDkZeCxgc='",
  ].join(" "),
  "style-src 'self' 'unsafe-inline'",
  "img-src 'self'",
  "font-src 'self'",
  "connect-src 'self'",
  "manifest-src 'self'",
  "object-src 'none'",
  "base-uri 'none'",
  "form-action 'none'",
  "frame-src 'none'",
  "frame-ancestors 'none'",
  "upgrade-insecure-requests",
].join("; ");

function withDocsHeaders(response) {
  const headers = new Headers(response.headers);
  headers.set("content-security-policy", CONTENT_SECURITY_POLICY);
  headers.set("x-content-type-options", "nosniff");
  headers.set("referrer-policy", "strict-origin-when-cross-origin");
  headers.set("permissions-policy", "interest-cohort=()");
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

export default {
  // Every branch goes through `withDocsHeaders`, including the two that answer
  // without touching the asset store. They used to not, and the reason to fix
  // it while adding a policy is that a reader checking whether the site sends
  // one would find two paths where it does not and no rule saying which paths
  // those are. One wrapper, every answer.
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/setup" || url.pathname === "/setup/") {
      return withDocsHeaders(Response.redirect(SETUP_URL, 302));
    }

    if (url.pathname === "/api/health") {
      return withDocsHeaders(
        Response.json({
          ok: true,
          service: "uf-docs",
        }),
      );
    }

    return withDocsHeaders(await env.ASSETS.fetch(request));
  },
};
