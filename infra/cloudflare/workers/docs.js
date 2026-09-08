const SETUP_URL = "https://setup.uniflowed.dev";

/**
 * The SHA-256 of the one inline script the site serves.
 *
 * `docs/app/_design/theme.js` exports `themeBootstrap`, which the layout
 * inlines into `<head>`: it reads the stored theme and sets `data-theme`
 * before first paint, because a stored dark preference applied after the first
 * frame is a white flash and there is no CSS that can express "read
 * localStorage". It has to be inline and synchronous, so the policy names it
 * by hash rather than allowing inline script at all.
 *
 * `docs-csp.test.js` computes this from the exported string and fails when the
 * two drift — which is the only thing standing between a change to that script
 * and a site whose theme bootstrap is silently blocked.
 */
const THEME_BOOTSTRAP_HASH = "sha256-GfUNcJf52UpaQd6LU2T1KTtYHa1CrhXX4vgwslYyF14=";

/**
 * What the site is allowed to load, derived from what it actually loads.
 *
 * Every directive here was read off the build rather than copied from a
 * template: the site serves same-origin scripts, stylesheets and images and
 * nothing else — no fonts, no `url()` in any stylesheet, no `data:` URI, no
 * `fetch` to another origin, no frame of any kind. So every fetch directive is
 * `'self'` and the ones for things it does not do at all are `'none'`.
 *
 * `style-src` needs no `'unsafe-inline'`, which is worth saying out loud: the
 * build emits no inline `style` attribute and no `<style>` element, so the
 * commonest reason a CSP is weakened does not apply here.
 *
 * `<script type="application/ld+json">` is a data block rather than a script —
 * the HTML parser never executes it — so it needs no hash of its own, which is
 * as well, since its content differs per page.
 *
 * The documentation site is where uf argues about security, and
 * `docs/security.md` has a row about CSP. See ubugeeei-prod/uf#598.
 */
const CONTENT_SECURITY_POLICY = [
  "default-src 'self'",
  `script-src 'self' '${THEME_BOOTSTRAP_HASH}'`,
  "style-src 'self'",
  "img-src 'self'",
  "font-src 'self'",
  "connect-src 'self'",
  "manifest-src 'self'",
  // Nothing the site does needs these, and saying so is what makes an
  // injected `<object>` or a framed copy of the site fail rather than work.
  "object-src 'none'",
  "frame-src 'none'",
  "frame-ancestors 'none'",
  "base-uri 'self'",
  "form-action 'self'",
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

export { CONTENT_SECURITY_POLICY, THEME_BOOTSTRAP_HASH, withDocsHeaders };

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/setup" || url.pathname === "/setup/") {
      return Response.redirect(SETUP_URL, 302);
    }

    if (url.pathname === "/api/health") {
      return Response.json({
        ok: true,
        service: "uf-docs",
      });
    }

    return withDocsHeaders(await env.ASSETS.fetch(request));
  },
};
