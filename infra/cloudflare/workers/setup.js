// GitHub Releases is where the release workflow publishes, and it is what
// install.sh downloads from by default. Point metadata at the same place so the
// two cannot disagree about what "latest" is.
const RELEASE_BASE_URL = "https://github.com/ubugeeei-prod/uf/releases/latest/download";
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
      const upstream = await fetch(`${RELEASE_BASE_URL}/manifest.json`, {
        headers: { accept: "application/json" },
        redirect: "follow",
      });
      if (!upstream.ok) {
        // Say so in the body rather than passing an HTML error page off as the
        // manifest; clients parse this as JSON.
        return Response.json(
          { error: "no published release", status: upstream.status },
          {
            status: 502,
            headers: {
              "access-control-allow-origin": "*",
              "cache-control": "no-store",
            },
          },
        );
      }
      return responseWithHeaders(upstream, {
        "content-type": "application/json; charset=utf-8",
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
