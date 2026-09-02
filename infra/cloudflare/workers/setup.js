const RELEASE_BASE_URL = "https://releases.uniflowed.dev/uf";
const DOCS_INSTALL_URL = "https://docs.uniflowed.dev/install";

function responseWithHeaders(response, headers) {
  const nextHeaders = new Headers(response.headers);
  for (const [name, value] of Object.entries(headers)) {
    nextHeaders.set(name, value);
  }
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers: nextHeaders,
  });
}

async function asset(pathname, request, env) {
  const url = new URL(request.url);
  url.pathname = pathname;
  url.search = "";
  return env.ASSETS.fetch(new Request(url, { method: "GET" }));
}

export default {
  async fetch(request, env) {
    const url = new URL(request.url);

    if (url.pathname === "/health") {
      return Response.json({
        ok: true,
        service: "uf-setup",
        releaseBaseUrl: RELEASE_BASE_URL,
      });
    }

    if (url.pathname === "/metadata/latest.json") {
      const upstream = await fetch(`${RELEASE_BASE_URL}/latest/manifest.json`, {
        headers: { accept: "application/json" },
      });
      return responseWithHeaders(upstream, {
        "access-control-allow-origin": "*",
        "cache-control": "public, max-age=60",
      });
    }

    if (url.pathname === "/" || url.pathname === "/install.sh") {
      const installer = await asset("/install.sh", request, env);
      return responseWithHeaders(installer, {
        "content-type": "text/x-shellscript; charset=utf-8",
        "cache-control": "public, max-age=300",
        "x-content-type-options": "nosniff",
      });
    }

    if (url.pathname === "/install.ps1") {
      const installer = await asset("/install.ps1", request, env);
      return responseWithHeaders(installer, {
        "content-type": "text/plain; charset=utf-8",
        "cache-control": "public, max-age=300",
        "x-content-type-options": "nosniff",
      });
    }

    if (url.pathname === "/docs" || url.pathname === "/docs/") {
      return Response.redirect(DOCS_INSTALL_URL, 302);
    }

    return new Response("not found\n", {
      status: 404,
      headers: {
        "content-type": "text/plain; charset=utf-8",
        "cache-control": "public, max-age=60",
      },
    });
  },
};
