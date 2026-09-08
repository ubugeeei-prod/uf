// @flow
//
// The headers the documentation site serves, and the one thing that keeps its
// content security policy true.
//
// The site is where uf argues about security — `docs/security.md` has a row
// about CSP — and it served no `content-security-policy` at all. See
// ubugeeei-prod/uf#598.
//
// A policy is only worth having if something notices when it stops matching
// the site. The part that can drift is the hash: the layout inlines
// `themeBootstrap` into `<head>` because a stored dark theme applied after the
// first frame is a white flash, and an inline script can only be allowed by
// hash without weakening `script-src` to `'unsafe-inline'`. So the hash is
// computed here from the exported string and compared with the worker's,
// which is what turns "somebody edited the bootstrap" from a silently broken
// theme into a failing test.

import { createHash } from "node:crypto";

import { describe, expect, it } from "@uniflowed/test";
// By path: a Cloudflare worker is a deployment artefact rather than a package,
// and `docs/app` is an application. Neither is anybody's dependency.
import { themeBootstrap } from "../../docs/app/_design/theme.js";
import worker, {
  CONTENT_SECURITY_POLICY,
  THEME_BOOTSTRAP_HASH,
  withDocsHeaders,
} from "../../infra/cloudflare/workers/docs.js";

/** The policy as `{ directive: [source, …] }`, which is how it is asked about. */
function directives(policy: string): { [string]: Array<string> } {
  const parsed: { [string]: Array<string> } = {};
  for (const part of policy.split(";")) {
    const [name, ...sources] = part.trim().split(/\s+/);
    if (name !== "") {
      parsed[name] = sources;
    }
  }
  return parsed;
}

describe("the inline theme bootstrap is allowed by hash", () => {
  it("hashes to what the worker's policy names", () => {
    const hash = `sha256-${createHash("sha256").update(themeBootstrap).digest("base64")}`;
    expect(hash).toBe(THEME_BOOTSTRAP_HASH);
  });

  it("names that hash in script-src", () => {
    expect(directives(CONTENT_SECURITY_POLICY)["script-src"]).toContain(
      `'${THEME_BOOTSTRAP_HASH}'`,
    );
  });
});

describe("the policy allows what the site loads and nothing else", () => {
  const parsed = directives(CONTENT_SECURITY_POLICY);

  it("allows script and style from the site itself", () => {
    expect(parsed["script-src"]).toContain("'self'");
    expect(parsed["style-src"]).toEqual(["'self'"]);
    expect(parsed["default-src"]).toEqual(["'self'"]);
  });

  it("needs no unsafe-inline anywhere, and no unsafe-eval", () => {
    // The commonest reason a policy is weakened is an inline `style`
    // attribute, and this build emits none — so the absence is a fact about
    // the site rather than an aspiration.
    expect(CONTENT_SECURITY_POLICY).not.toContain("unsafe-inline");
    expect(CONTENT_SECURITY_POLICY).not.toContain("unsafe-eval");
  });

  it("refuses the things the site never does", () => {
    // A framed copy of the site, and an injected `<object>`, both fail.
    expect(parsed["object-src"]).toEqual(["'none'"]);
    expect(parsed["frame-src"]).toEqual(["'none'"]);
    expect(parsed["frame-ancestors"]).toEqual(["'none'"]);
  });

  it("pins the base URI and where a form may post", () => {
    // Without `base-uri`, one injected `<base>` redirects every relative
    // script URL in the document to another origin, and `script-src 'self'`
    // does not stop it.
    expect(parsed["base-uri"]).toEqual(["'self'"]);
    expect(parsed["form-action"]).toEqual(["'self'"]);
  });

  it("allows no other origin for anything it fetches", () => {
    for (const [name, sources] of Object.entries(parsed)) {
      for (const source of sources) {
        expect(
          source.startsWith("'") || source === "'self'",
          `${name} names an origin: ${source}`,
        ).toBe(true);
      }
    }
  });
});

describe("every response carries the headers", () => {
  it("sets the policy and the other three", () => {
    const response = withDocsHeaders(new Response("<!doctype html>", { status: 200 }));
    expect(response.headers.get("content-security-policy")).toBe(CONTENT_SECURITY_POLICY);
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
    expect(response.headers.get("referrer-policy")).toBe("strict-origin-when-cross-origin");
    expect(response.headers.get("permissions-policy")).toBe("interest-cohort=()");
  });

  it("keeps the status and the body it was given", async () => {
    const response = withDocsHeaders(new Response("not found", { status: 404 }));
    expect(response.status).toBe(404);
    expect(await response.text()).toBe("not found");
  });

  it("sets them on an asset the way it does on a page", async () => {
    // The asset path is the one that matters most: a stylesheet served
    // without `x-content-type-options` is the sniffing case, and a policy
    // that only reached HTML would be a policy on one response in ten.
    const env = { ASSETS: { fetch: async () => new Response("body{}", { status: 200 }) } };
    const response = await worker.fetch(
      new Request("https://docs.uniflowed.dev/brand/tokens.css"),
      env,
    );
    expect(response.headers.get("content-security-policy")).toBe(CONTENT_SECURITY_POLICY);
  });
});
