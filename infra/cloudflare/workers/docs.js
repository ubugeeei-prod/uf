const SETUP_URL = "https://setup.uniflowed.dev";

function withDocsHeaders(response) {
  const headers = new Headers(response.headers);
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
