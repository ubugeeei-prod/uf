// @flow
//
// Internal to `@uniflowed/server`: refusing a browser that is on another build.
//
// A tab opened on build N holds build N's client: its action ids, its chunk
// names, and a router that renders payloads against both. When build N+1 is
// what answers, two of the things that tab can ask for are no longer
// questions this build can answer honestly. An action id is an HMAC over a
// per-build secret, so N's ids name nothing here; and a route's payload names
// N+1's client chunks, which N's page would load beside its own copy of
// React. Neither fails loudly on its own — the first is a 404 the page shows
// as a broken button, the second is a page that renders with two Reacts.
//
// So a browser says which build it is on, and a build that is not that one
// answers `409` instead of running anything. `@uniflowed/router` treats that
// answer as "load the document": the browser makes a hard navigation, gets
// build N+1's document, and is on N+1 from then on. The whole contract is
// written down in `docs/app/guide/deploy/$page.mdx`, under version skew.
//
// # What the id is
//
// Not the build id. That one keys the action HMAC and the durable cache, and
// `docs/app/guide/server-actions` promises it is never published. The
// deployment id is a digest of it (`@uniflowed/vite`'s `deploymentIdFor`):
// the same for two artefacts that are one build, different for any two that
// are not, and saying nothing about the secret it came from. It is in every
// document the build writes, as `<meta name="uf:deployment">`, so a visitor
// can read it and nothing is lost by that.
//
// # Where the check sits
//
// Before the application and after the files. A file is answered by every
// host before this runs, which is what keeps a tab on build N working at all:
// N's chunks are still on disk for one more build (`uf build` carries them
// forward), and a chunk request carries no header — an `import()` cannot send
// one. Everything after the files is the application, and nothing in it runs
// for a request this refuses: not the middleware, not the action, not the
// render.

/** The header a browser names its build in, and a refusal names this one in. */
export const DEPLOYMENT_HEADER = "uf-deployment";

/**
 * The refusal for a request from another build, or `null` to carry on.
 *
 * `null` whenever there is nothing to compare: a build with no deployment id
 * (`uf dev`, or a build from before there was one) and a request that names
 * none (`curl`, a crawler, a browser that has not loaded a uf page). The check
 * is for a browser that says which build it is on and is wrong about it, and
 * for nothing else.
 */
export function refuseOtherDeployment(request: Request, current: ?string): Response | null {
  if (current == null || current === "") return null;
  const named = request.headers.get(DEPLOYMENT_HEADER);
  if (named == null || named === "" || named === current) return null;
  const headers = new Headers({
    "content-type": "text/plain; charset=utf-8",
    // Never kept by anything between here and the browser. A cached refusal
    // would send the next tab on the *current* build to a hard navigation too.
    "cache-control": "no-store",
  });
  headers.set(DEPLOYMENT_HEADER, current);
  return new Response("409 Conflict: this page is from another deployment; load it again\n", {
    status: 409,
    headers,
  });
}
