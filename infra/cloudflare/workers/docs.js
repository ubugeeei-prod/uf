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

    if (url.pathname === "/install" || url.pathname === "/install/") {
      return new Response(
        [
          "# Install uf",
          "",
          "## curl",
          "",
          "```sh",
          "curl -fsSL https://setup.uniflowed.dev | sh",
          "```",
          "",
          "## Nix",
          "",
          "```sh",
          "nix run github:ubugeeei-prod/uf#uf -- --version",
          "nix profile install github:ubugeeei-prod/uf#uf",
          "```",
          "",
        ].join("\n"),
        {
          headers: {
            "content-type": "text/markdown; charset=utf-8",
            "cache-control": "public, max-age=300",
          },
        },
      );
    }

    if (url.pathname === "/setup" || url.pathname === "/setup/") {
      return Response.redirect(SETUP_URL, 302);
    }

    return withDocsHeaders(await env.ASSETS.fetch(request));
  },
};
