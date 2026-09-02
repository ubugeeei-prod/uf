const DOCS_HOST = "docs.uniflowed.dev";

export default {
  fetch(request) {
    const url = new URL(request.url);
    url.hostname = DOCS_HOST;
    return Response.redirect(url.toString(), 308);
  },
};
