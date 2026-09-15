// @flow
//
// Internal to `@uniflowed/router`: a route's metadata, as the elements React
// hoists into `<head>`.
//
// Markup and nothing else — no hooks, no context — so the same component renders
// a route's head in the tree a server composes for React Server Components and
// in the tree the browser composes for a single-page application. `useSeo` in
// `./runtime.js` is the component-level way in, and it renders this too.

import * as React from "react";

import type { JsonLd, Metadata, Robots } from "./resolve.js";

/**
 * One URL from a route's metadata, made absolute if it can be.
 *
 * Open Graph, Twitter and `rel="canonical"` all want an absolute URL, and a
 * route module cannot know the host it is served from — so `metadataBase` is
 * how a site says it once, and this is where it is applied.
 *
 * Three things it deliberately does not do. It does not resolve against the
 * *page's* URL: `Head` renders inside the route and does not know it, and a
 * `metadataBase` is a site-wide fact rather than a per-page one. It does not
 * invent a base: with none declared the value is emitted exactly as written,
 * which is what every page that predates this field already gets. And it does
 * not throw — a `metadataBase` that is not a URL is a mistake in one field,
 * and turning it into a blank page would be a worse answer than an unresolved
 * `og:image`.
 */
function absoluteUrl(value: string, base: void | string): string {
  if (base == null) return value;
  try {
    return new URL(value, base).href;
  } catch {
    return value;
  }
}

/**
 * The `robots` directives, as one `content` string, or `null` for none.
 *
 * `null` rather than an empty string, so a page that declared nothing gets no
 * tag at all: "index, follow" is what a document with no `robots` meta already
 * means, and writing it out tells a crawler what it had already assumed.
 *
 * Each declared field contributes its directive and no field implies another.
 * `index: true` therefore emits `index` rather than nothing — the value is
 * there to overrule a section that said otherwise, and a directive that
 * disappeared because it agreed with the default would be a page saying
 * something and no evidence of it in the markup.
 */
function robotsContent(robots: void | Robots): ?string {
  if (robots == null) {
    return null;
  }
  const directives: Array<string> = [];
  if (robots.index != null) {
    directives.push(robots.index ? "index" : "noindex");
  }
  if (robots.follow != null) {
    directives.push(robots.follow ? "follow" : "nofollow");
  }
  if (robots.maxSnippet != null) {
    directives.push(`max-snippet:${robots.maxSnippet}`);
  }
  if (robots.maxImagePreview != null) {
    directives.push(`max-image-preview:${robots.maxImagePreview}`);
  }
  return directives.length === 0 ? null : directives.join(", ");
}

/**
 * One JSON-LD object as the text of a `<script>`.
 *
 * `<` is escaped so a string inside the data holding `</script>` cannot end
 * the element early — the same escape `server.js` applies to the embedded
 * loader data, and for the same reason: the text is the application's and the
 * element it lands in is terminated by a character sequence rather than by a
 * length. `dataScript` also escapes U+2028 and U+2029; those are about a
 * string being parsed as JavaScript source, and this one never is.
 */
function jsonLdText(entry: JsonLd): string {
  return JSON.stringify(entry).replace(/</g, "\\u003c");
}

/**
 * One JSON-LD object, as the element that carries it.
 *
 * A function rather than an element written inline, because the suppression
 * needs a line of its own; `docs/app/$layout.js` has the same shape for the
 * same reason. `security/no-dangerously-set-inner-html` is about markup that
 * came from somewhere and has to be sanitized before a browser parses it as
 * HTML, and its escape hatch is a `@uniflowed/markdown` sanitizer — the right
 * answer for markup and no answer at all for JSON. This string is
 * `JSON.stringify`'s output with `<` escaped, so nothing in it can close the
 * element, and it is never parsed as HTML. There is also no other spelling:
 * React escapes a text child, so `{"@type":"Article"}` would reach the page as
 * `&quot;@type&quot;`, which is not JSON-LD any more.
 */
function jsonLdScript(entry: JsonLd): React.Node {
  const text = jsonLdText(entry);
  const html = { __html: text };
  // uf-lint-disable-next-line security/no-dangerously-set-inner-html
  return <script key={text} type="application/ld+json" dangerouslySetInnerHTML={html} />;
}

export component Head(metadata: Metadata) {
  const { title, description, metadataBase, canonical, robots } = metadata;
  const { alternates, pagination, jsonLd, openGraph, twitter } = metadata;
  const href = canonical != null ? absoluteUrl(canonical, metadataBase) : null;
  const crawler = robotsContent(robots);
  // Read out of `alternates` once rather than through it at every use: the map
  // is read inside a callback, and a refinement of `alternates.languages` does
  // not survive being carried into one.
  const languages = alternates?.languages;
  // A page that said what it is called has said what its card is called. Every
  // site that had to write both wrote the same string twice, and the second
  // one is the one that goes stale — the docs site shipped thirty pages whose
  // share cards carried an image and no title at all.
  //
  // `??`, not `||`: an empty string is a decision, and a page that deliberately
  // has no card title should get none rather than the document's.
  const cardTitle = openGraph?.title ?? title;
  const cardDescription = openGraph?.description ?? description;
  // `og:type` is one of the four properties Open Graph requires. A default is
  // the difference between a document with a card and a document without one,
  // and `website` is right for everything that is not an article or a video.
  const cardType = openGraph?.type ?? "website";
  // Only when the card was asked for. A page with no `twitter.card` gets no
  // Twitter tags at all, which is what a site that never wanted one meant.
  const twitterTitle = twitter != null ? (twitter.title ?? cardTitle) : null;
  const twitterDescription = twitter != null ? (twitter.description ?? cardDescription) : null;
  const twitterImageAlt = twitter != null ? (twitter.imageAlt ?? openGraph?.imageAlt) : null;
  return (
    <>
      {title != null ? <title>{title}</title> : null}
      {description != null ? <meta name="description" content={description} /> : null}
      {crawler != null ? <meta name="robots" content={crawler} /> : null}
      {href != null ? <link rel="canonical" href={href} /> : null}
      {/* The set is reciprocal and includes this page, so a `hreflang` list is
          usually the same list on every page of it — which is why it belongs
          on the layout they share rather than on each of them.

          `hrefLang` is React's spelling and it reaches the markup unchanged,
          which is worth knowing before grepping a document for `hreflang` and
          concluding it is missing. HTML attribute names are case-insensitive,
          so the parser every crawler runs reads it as the same attribute; the
          lowercase spelling is the one React warns about. */}
      {languages != null
        ? Object.keys(languages).map((language) => (
            <link
              key={language}
              rel="alternate"
              hrefLang={language}
              href={absoluteUrl(languages[language], metadataBase)}
            />
          ))
        : null}
      {pagination?.prev != null ? (
        <link rel="prev" href={absoluteUrl(pagination.prev, metadataBase)} />
      ) : null}
      {pagination?.next != null ? (
        <link rel="next" href={absoluteUrl(pagination.next, metadataBase)} />
      ) : null}
      {/* `og:url` *is* the canonical URL of the page, in Open Graph's own
          words, so one declaration answers both rather than asking a project
          to write the same URL twice and keep them in step. */}
      {href != null ? <meta property="og:url" content={href} /> : null}
      {cardTitle != null ? <meta property="og:title" content={cardTitle} /> : null}
      {cardDescription != null ? (
        <meta property="og:description" content={cardDescription} />
      ) : null}
      {/* Only alongside something else. A document with `og:type` and nothing
          more is not a card; it is one meta tag saying the page is a page. */}
      {cardTitle != null || cardDescription != null || openGraph?.images != null ? (
        <meta property="og:type" content={cardType} />
      ) : null}
      {openGraph?.siteName != null ? (
        <meta property="og:site_name" content={openGraph.siteName} />
      ) : null}
      {openGraph?.images != null
        ? openGraph.images.map((image) => (
            <meta key={image} property="og:image" content={absoluteUrl(image, metadataBase)} />
          ))
        : null}
      {openGraph?.imageAlt != null && openGraph?.images != null ? (
        <meta property="og:image:alt" content={openGraph.imageAlt} />
      ) : null}
      {/* `name`, not `property`: Open Graph is RDFa and Twitter's cards are
          not, and a `property="twitter:card"` is ignored by the crawler that
          reads it. */}
      {twitter?.card != null ? <meta name="twitter:card" content={twitter.card} /> : null}
      {twitter?.site != null ? <meta name="twitter:site" content={twitter.site} /> : null}
      {twitter?.creator != null ? <meta name="twitter:creator" content={twitter.creator} /> : null}
      {/* X reads the `og:` tags when these are absent, so these are not
          required — and every validator asks for them anyway, which is a good
          enough reason when the value is one the page has already given. They
          fall back through the card's title to the document's. */}
      {twitterTitle != null ? <meta name="twitter:title" content={twitterTitle} /> : null}
      {twitterDescription != null ? (
        <meta name="twitter:description" content={twitterDescription} />
      ) : null}
      {twitter?.images != null
        ? twitter.images.map((image) => (
            <meta key={image} name="twitter:image" content={absoluteUrl(image, metadataBase)} />
          ))
        : null}
      {twitterImageAlt != null && twitter?.images != null ? (
        <meta name="twitter:image:alt" content={twitterImageAlt} />
      ) : null}
      {/* Last, and not hoisted into `<head>` with the rest: React hoists a
          `<title>`, a `<meta>` and a `<link>`, and not a script whose body it
          would have to carry. JSON-LD is read from anywhere in the document,
          so these render where the route does. */}
      {jsonLd != null ? jsonLd.map(jsonLdScript) : null}
    </>
  );
}
