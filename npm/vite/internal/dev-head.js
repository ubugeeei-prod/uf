// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// The head `uf dev` hands to Vite's `transformIndexHtml`, made into the kind of
// document that hook was written for, and put back into the shape React
// hydrates against afterwards.
//
// Vite's development hook is written for an `index.html` a person wrote. Two
// of its assumptions are wrong about a head React rendered, and each showed up
// as a page that hydrates against markup the application never wrote:
//
//   * **Its absolute URLs leave the base out.** The hook joins `config.base`
//     onto every root-relative `src`, `href` and `srcset` it knows, so a head
//     that already carries the base — uf's own stylesheet links, and anything
//     built from `import.meta.env.BASE_URL` — came out with it twice under
//     `app.router.basePath`. See ubugeeei-prod/uf#1677.
//   * **Nothing in the head is compared with anything.** Vite's client and
//     uf's preambles go in with `head-prepend`, ahead of the layout's own
//     children, and React 19 pairs a `<script>` it is hydrating with the first
//     `<script>` it finds there. A layout's `<script src="/theme.js">` met
//     `/@vite/client`. See ubugeeei-prod/uf#1682.
//
// So the base comes off every URL the hook is about to put it on, and every
// tag the hook prepended is moved to the end of the head, after everything the
// layout rendered and before uf's own client entry. React has claimed each of
// the layout's head children by the time it gets there, and a singleton's
// trailing nodes are never compared. The client entry still comes last, so
// the order the scripts run in is the order they always ran in.

/**
 * `server.transformIndexHtml(url, html)`, for a document React rendered.
 *
 * `entry` is the URL of uf's client entry exactly as the document shell wrote
 * it — before the hook, so without the base. The prepended tags go right before
 * its `<script>`, which keeps them ahead of every module the document runs.
 */
export async function transformRenderedHead(server, url, html, entry) {
  const marked = markDevHead(
    stripBaseForDevHook(html, server.config.base),
    `<script type="module" src="${entry}"`,
  );
  return settleDevHead(await server.transformIndexHtml(url, marked));
}

/**
 * Where Vite's development hook rewrites a URL, as `DEFAULT_HTML_ASSET_SOURCES`
 * in Vite's `html.ts` lists them, plus every `<script src>`.
 *
 * An attribute that is not in this list is one the hook leaves alone, so the
 * base must be left on it too: taking it off a URL nothing puts it back on
 * would be the same bug in the other direction.
 */
const ASSET_SOURCES = {
  audio: { src: ["src"] },
  embed: { src: ["src"] },
  img: { src: ["src"], srcset: ["srcset"] },
  image: { src: ["href", "xlink:href"] },
  input: { src: ["src"] },
  link: { src: ["href"], srcset: ["imagesrcset"] },
  object: { src: ["data"] },
  script: { src: ["src"] },
  source: { src: ["src"], srcset: ["srcset"] },
  track: { src: ["src"] },
  use: { src: ["href", "xlink:href"] },
  video: { src: ["src", "poster"] },
  meta: { src: ["content"] },
};

/** The `<meta>` whose `content` Vite treats as a URL; the rest are text. */
const META_NAMES = new Set([
  "msapplication-tileimage",
  "msapplication-square70x70logo",
  "msapplication-square150x150logo",
  "msapplication-wide310x150logo",
  "msapplication-square310x310logo",
  "msapplication-config",
  "twitter:image",
]);
const META_PROPERTIES = new Set([
  "og:image",
  "og:image:url",
  "og:image:secure_url",
  "og:audio",
  "og:audio:secure_url",
  "og:video",
  "og:video:secure_url",
]);

/**
 * A comment, an element whose content is not markup, or an opening tag.
 *
 * The content of a `<script>`, `<style>`, `<title>` or `<textarea>` is skipped
 * whole: a string in an inline script that looks like `<link href="/docs/…">`
 * is not a link, and Vite's parser does not think it is one either.
 */
const MARKUP =
  /<!--[\s\S]*?-->|<(script|style|title|textarea)\b([^>]*)>[\s\S]*?<\/\1\s*>|<([a-zA-Z][\w:-]*)\b([^>]*)>/gi;

/** One attribute of an opening tag, as React and uf write them. */
const ATTRIBUTE = /(\s)([^\s"'>/=]+)(?:\s*=\s*(?:"([^"]*)"|'([^']*)'|([^\s"'=<>`]+)))?/g;

/**
 * `head` with `base` taken off every URL Vite's development hook will put it
 * back on.
 *
 * A URL that carries the base is one the hook would have given it twice; one
 * that does not — uf's client entry, `/@id/__x00__virtual:uf/client` — is
 * left for the hook to prefix, which is what it already did. `base` is Vite's
 * `config.base`, with its trailing slash; at `/` there is nothing to take off.
 */
export function stripBaseForDevHook(head, base) {
  if (typeof base !== "string" || base === "/" || !base.startsWith("/") || !base.endsWith("/")) {
    return head;
  }
  const strip = (url) => (url.startsWith(base) ? url.slice(base.length - 1) : url);
  const stripSet = (value) =>
    value.replace(new RegExp(`(^|,)(\\s*)${escapeRegExp(base)}`, "g"), "$1$2/");
  return head.replace(MARKUP, (match, rawName, rawAttrs, openName, openAttrs) => {
    const name = (rawName ?? openName)?.toLowerCase();
    const attrs = rawAttrs ?? openAttrs;
    const sources = name == null ? undefined : ASSET_SOURCES[name];
    if (sources == null || attrs == null) return match;
    const values = attributesOf(attrs);
    // `vite-ignore` is the element saying the hook should not touch it, and
    // the hook honours it; so does this.
    if (values.has("vite-ignore")) return match;
    if (name === "meta" && !isUrlMeta(values)) return match;
    const src = new Set(sources.src ?? []);
    const srcset = new Set(sources.srcset ?? []);
    const rewritten = attrs.replace(ATTRIBUTE, (attribute, space, key, double, single, bare) => {
      const lower = key.toLowerCase();
      const value = double ?? single ?? bare;
      if (value == null) return attribute;
      const next = src.has(lower) ? strip(value) : srcset.has(lower) ? stripSet(value) : value;
      if (next === value) return attribute;
      const quote = double != null ? '"' : single != null ? "'" : "";
      return `${space}${key}=${quote}${next}${quote}`;
    });
    // By position, not by `replace`: the attributes begin right after `<name`.
    const at = 1 + (rawName ?? openName).length;
    return match.slice(0, at) + rewritten + match.slice(at + attrs.length);
  });
}

function attributesOf(attrs) {
  const values = new Map();
  for (const [, , key, double, single, bare] of attrs.matchAll(ATTRIBUTE)) {
    values.set(key.toLowerCase(), double ?? single ?? bare ?? "");
  }
  return values;
}

function isUrlMeta(values) {
  const name = values.get("name")?.trim().toLowerCase();
  const property = values.get("property")?.trim().toLowerCase();
  return (
    (name != null && META_NAMES.has(name)) || (property != null && META_PROPERTIES.has(property))
  );
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/** Where the head-prepended tags end: written before the hook, gone after it. */
export const PREPENDED_MARK = "<!--uf:dev-head-prepended-->";

/** Where uf's own head tags begin, and so where the prepended ones go. */
export const ENTRY_MARK = "<!--uf:dev-head-entry-->";

const HEAD_OPEN = /<head(?:\s[^>]*)?>/i;

/**
 * `head`, marked so that `settleDevHead` can find what the hook prepended and
 * where it belongs.
 *
 * `entry` is the tag uf's own head tags begin with — the client entry's
 * `<script>`, or the form state before it — as the document shell wrote it.
 * When it is not in the head the prepended tags go before `</head>` instead.
 */
export function markDevHead(head, entry) {
  const open = head.match(HEAD_OPEN);
  if (open == null || open.index == null) return head;
  const after = open.index + open[0].length;
  let marked = `${head.slice(0, after)}${PREPENDED_MARK}${head.slice(after)}`;
  const at = entry == null ? -1 : marked.indexOf(entry, after + PREPENDED_MARK.length);
  if (at !== -1) marked = `${marked.slice(0, at)}${ENTRY_MARK}${marked.slice(at)}`;
  return marked;
}

/**
 * The transformed head, with everything the hook put ahead of the mark moved
 * to where uf's own tags begin.
 *
 * Moved rather than left for the browser to skip, because React 19 only steps
 * over an unexpected head element when its tag differs from the one it is
 * looking for: a `<script>` of Vite's ahead of a `<script>` of the layout's is
 * compared with it, attribute by attribute. After the layout's children there
 * is nothing left to compare them with.
 */
export function settleDevHead(html) {
  const mark = html.indexOf(PREPENDED_MARK);
  if (mark === -1) return html.replace(ENTRY_MARK, "");
  const open = html.match(HEAD_OPEN);
  const start = open == null || open.index == null ? mark : open.index + open[0].length;
  const prepended = start <= mark ? html.slice(start, mark) : "";
  let rest = html.slice(0, start) + html.slice(mark + PREPENDED_MARK.length);
  const entry = rest.indexOf(ENTRY_MARK);
  if (entry !== -1) {
    return rest.slice(0, entry) + prepended + rest.slice(entry + ENTRY_MARK.length);
  }
  const close = rest.search(/<\/head>/i);
  if (close === -1) return rest + prepended;
  rest = rest.slice(0, close) + prepended + rest.slice(close);
  return rest;
}
