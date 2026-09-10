// @flow
//
// What a route's `metadata` puts in the document.
//
// uf's metadata was five fields and five tags: a title, a description and
// three Open Graph ones. There was no canonical URL, no Twitter card, and —
// the part that made one of the five wrong rather than missing — no absolute
// base, so `openGraph.images: ["/og.png"]` shipped exactly as written and a
// relative `og:image` is not an Open Graph image at all. See
// ubugeeei-prod/uf#269.
//
// The assertions are over the rendered document rather than over the resolved
// metadata object, because the object was never the thing that was wrong. What
// a crawler reads is the markup, and `og:` against `property` and `twitter:`
// against `name` is the kind of detail that is either right in the renderer or
// silently ignored by the machine that reads it.

import * as React from "@uniflowed/react";
import { useRandom, useRenderedAt } from "@uniflowed/hooks";
import { routerView, useSeo } from "@uniflowed/router";
import type { LayoutModule, Metadata, PageModule } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { describe, expect, it } from "@uniflowed/test";

const assets = { scripts: [], styles: [], preloads: [] };

component Page() {
  return <p>a page</p>;
}

component SiteLayout(children: React.Node) {
  return <div className="site">{children}</div>;
}

/**
 * A root layout that renders the whole document, which is the other of the two
 * shapes `internal/stream.js` chooses between.
 */
component OwnDocument(children: React.Node) {
  return (
    <html lang="en">
      <head>
        <meta charSet="utf-8" />
      </head>
      <body>{children}</body>
    </html>
  );
}

/**
 * The document a single route with this metadata renders to.
 *
 * `layout` is how a test asks for the other document shape: with none, the app
 * renders only content and uf wraps it in the shell.
 */
async function documentFor(
  metadata: Metadata,
  layoutMetadata?: Metadata,
  layout?: React.ComponentType<empty>,
): Promise<string> {
  const Layout = layout ?? SiteLayout;
  const { prerender } = createRenderer({
    App: routerView("./app"),
    routes: [
      {
        path: "/guide",
        params: [],
        mdx: false,
        file: "app/guide/$page.js",
        page: () => Promise.resolve({ default: Page, metadata }),
        layouts:
          layoutMetadata == null && layout == null
            ? []
            : [() => Promise.resolve({ default: Layout, metadata: layoutMetadata })],
      },
    ],
    notFound: [],
    errors: [],
  });

  const result = await prerender("/guide", assets);
  // A throw anywhere above would render the error page instead, and every
  // `not.toContain` below would then pass for the wrong reason.
  expect(result.status).toBe(200);
  return result.html;
}

describe("a canonical URL", () => {
  it("is a link and an og:url, from one declaration", async () => {
    // `og:url` is defined as the canonical URL of the page, so asking a
    // project to write the same URL under two names would be asking it to keep
    // two copies of one fact in step.
    const html = await documentFor({ canonical: "https://docs.uniflowed.dev/guide" });

    expect(html).toContain('<link rel="canonical" href="https://docs.uniflowed.dev/guide"/>');
    expect(html).toContain('<meta property="og:url" content="https://docs.uniflowed.dev/guide"/>');
  });

  it("is resolved against metadataBase when it is a path", async () => {
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      canonical: "/guide",
    });

    expect(html).toContain('<link rel="canonical" href="https://docs.uniflowed.dev/guide"/>');
  });

  it("is absent from a page that declares none", async () => {
    const html = await documentFor({ title: "no canonical here" });

    expect(html).not.toContain('rel="canonical"');
    expect(html).not.toContain("og:url");
  });
});

describe("the card a page gets without asking for one", () => {
  it("takes og:title and og:description from the page's own title and description", async () => {
    // The bug: the docs site declared a title, a description and an image, and
    // shipped thirty pages whose share card was an image with no title on it.
    // A page that has said what it is called has said what its card is called.
    const html = await documentFor({
      title: "Install · uf",
      description: "One binary, three runtimes, no plugins to add.",
      metadataBase: "https://docs.uniflowed.dev",
      openGraph: { images: ["/brand/uf.png"] },
    });

    expect(html).toContain('<meta property="og:title" content="Install · uf"/>');
    expect(html).toContain(
      '<meta property="og:description" content="One binary, three runtimes, no plugins to add."/>',
    );
  });

  it("prefers what openGraph says, when it says anything", async () => {
    // No apostrophes: the renderer escapes them, and an assertion that fails
    // over `&#x27;` is an assertion about escaping written by accident.
    const html = await documentFor({
      title: "Install · uf",
      description: "the document",
      openGraph: { title: "Install uf", description: "the card" },
    });

    expect(html).toContain('<meta property="og:title" content="Install uf"/>');
    expect(html).toContain('<meta property="og:description" content="the card"/>');
    expect(html).not.toContain('<meta property="og:description" content="the document"/>');
  });

  it("declares og:type, which Open Graph requires and nobody remembers", async () => {
    const html = await documentFor({ title: "a page" });

    expect(html).toContain('<meta property="og:type" content="website"/>');
  });

  it("takes a type a page names instead", async () => {
    const html = await documentFor({
      title: "a post",
      openGraph: { type: "article" },
    });

    expect(html).toContain('<meta property="og:type" content="article"/>');
  });

  it("emits no og:type for a page with no card at all", async () => {
    // A document whose only Open Graph property says the page is a page is
    // not a card, and a crawler reading one learns nothing from it.
    const html = await documentFor({});

    expect(html).not.toContain("og:type");
  });

  it("describes the image, for a reader who cannot see it", async () => {
    const html = await documentFor({
      title: "a page",
      openGraph: { images: ["/og.png"], imageAlt: "the uf mark" },
      twitter: { card: "summary_large_image", images: ["/og.png"] },
    });

    expect(html).toContain('<meta property="og:image:alt" content="the uf mark"/>');
    // And the Twitter card takes the same description rather than asking for
    // it twice.
    expect(html).toContain('<meta name="twitter:image:alt" content="the uf mark"/>');
  });

  it("gives the Twitter card the title the page already has", async () => {
    // X reads the `og:` tags when these are absent, so they are not required —
    // and every validator asks for them, which is reason enough when the value
    // is one the page has already given.
    const html = await documentFor({
      title: "Install · uf",
      description: "One binary, three runtimes.",
      twitter: { card: "summary_large_image" },
    });

    expect(html).toContain('<meta name="twitter:title" content="Install · uf"/>');
    expect(html).toContain(
      '<meta name="twitter:description" content="One binary, three runtimes."/>',
    );
  });

  it("gives a page that asked for no card no Twitter tags at all", async () => {
    const html = await documentFor({ title: "a page", description: "a description" });

    expect(html).not.toContain("twitter:");
  });

  it("names the site once, from the layout, for every page under it", async () => {
    const html = await documentFor({ title: "Install · uf" }, { openGraph: { siteName: "uf" } });

    expect(html).toContain('<meta property="og:site_name" content="uf"/>');
  });
});

describe("metadataBase", () => {
  it("makes a relative og:image an absolute one, which is the only kind there is", async () => {
    // The bug this field exists for. A relative `og:image` is not resolved by
    // the crawlers that read it, so the card had no image and nothing said so.
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      openGraph: { images: ["/brand/uf.png"] },
    });

    expect(html).toContain(
      '<meta property="og:image" content="https://docs.uniflowed.dev/brand/uf.png"/>',
    );
  });

  it("leaves an absolute image alone", async () => {
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      openGraph: { images: ["https://cdn.example.com/og.png"] },
    });

    expect(html).toContain('<meta property="og:image" content="https://cdn.example.com/og.png"/>');
  });

  it("is inherited from a layout, so a site declares its origin once", async () => {
    const html = await documentFor(
      { openGraph: { images: ["/og.png"] } },
      { metadataBase: "https://docs.uniflowed.dev" },
    );

    expect(html).toContain(
      '<meta property="og:image" content="https://docs.uniflowed.dev/og.png"/>',
    );
  });

  it("changes nothing for a page that declares none", async () => {
    // Every page written before this field existed keeps the document it had.
    const html = await documentFor({ openGraph: { images: ["/og.png"] } });

    expect(html).toContain('<meta property="og:image" content="/og.png"/>');
  });

  it("does not take the document down when it is not a URL", async () => {
    // A mistake in one field is a mistake in one field. The alternative — a
    // throw out of `Head` — is a blank page for a bad `og:image`.
    const html = await documentFor({
      metadataBase: "not a url",
      openGraph: { images: ["/og.png"] },
    });

    expect(html).toContain('<meta property="og:image" content="/og.png"/>');
    expect(html).toContain("a page");
  });
});

describe("a Twitter card", () => {
  it("is emitted under name, which is the half that is easy to get wrong", async () => {
    // Open Graph is RDFa and uses `property`; Twitter's cards are not and use
    // `name`. A `property="twitter:card"` is ignored by the crawler that reads
    // it, with no error anywhere.
    const html = await documentFor({
      twitter: {
        card: "summary_large_image",
        site: "@uniflowed",
        creator: "@ubugeeei",
        title: "The manual",
        description: "Everything uf does.",
      },
    });

    expect(html).toContain('<meta name="twitter:card" content="summary_large_image"/>');
    expect(html).toContain('<meta name="twitter:site" content="@uniflowed"/>');
    expect(html).toContain('<meta name="twitter:creator" content="@ubugeeei"/>');
    expect(html).toContain('<meta name="twitter:title" content="The manual"/>');
    expect(html).toContain('<meta name="twitter:description" content="Everything uf does."/>');
    expect(html).not.toContain('property="twitter:card"');
  });

  it("resolves its images against metadataBase too", async () => {
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      twitter: { card: "summary", images: ["/brand/uf.png"] },
    });

    expect(html).toContain(
      '<meta name="twitter:image" content="https://docs.uniflowed.dev/brand/uf.png"/>',
    );
  });

  it("is absent from a page that declares none", async () => {
    const html = await documentFor({ title: "no card here" });

    expect(html).not.toContain("twitter:");
  });
});

// ---------------------------------------------------------------------------
// Where the tags land, which is a different question from what they say
// ---------------------------------------------------------------------------
//
// Every assertion above is a `toContain` over the document, and a `toContain`
// passes wherever in the document the tag is. `Head` relies on React hoisting a
// `<title>`, a `<meta>` and a `<link>` into `<head>`, and React does that for a
// document *it* rendered — an app whose root layout writes `<html>`. uf's own
// shell is not one, so with it every tag landed inside `<div id="uf-root">`,
// where a crawler ignores the canonical link and the share card has no title.
// Nothing said so, and the difference between the two shapes is what makes it
// worth a section of its own. See ubugeeei-prod/uf#547.

/** Whether `tag` appears in the document's head rather than in its body. */
function inHead(html: string, tag: string): boolean {
  const at = html.indexOf(tag);
  return at !== -1 && at < html.indexOf("</head>");
}

describe("the head of a document uf wrote the shell for", () => {
  it("carries the canonical link, which is worth nothing in the body", async () => {
    // Google ignores a `rel="canonical"` outside the head, so a project on the
    // shell was quietly getting worse SEO than one that had written six lines
    // of `<html>` — with nothing anywhere to say which it had.
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      canonical: "/guide",
    });

    expect(inHead(html, '<link rel="canonical"')).toBe(true);
    expect(html).toContain('id="uf-root"');
  });

  it("carries the share card too, and the description", async () => {
    const html = await documentFor({
      title: "The manual",
      description: "Everything uf does.",
      openGraph: { images: ["https://docs.uniflowed.dev/og.png"] },
      twitter: { card: "summary_large_image" },
    });

    expect(inHead(html, '<meta name="description"')).toBe(true);
    expect(inHead(html, 'property="og:title"')).toBe(true);
    expect(inHead(html, 'property="og:image"')).toBe(true);
    expect(inHead(html, 'name="twitter:card"')).toBe(true);
  });

  it("carries exactly one title, from the metadata rather than from two places", async () => {
    // The shell wrote a `<title>` of its own from `resolved.metadata.title`,
    // which is the same string `Head` renders — so a document had two, and only
    // the one in the head was where a browser looks.
    const html = await documentFor({ title: "The manual" });

    expect(inHead(html, "<title>The manual</title>")).toBe(true);
    expect(html.split("<title>").length - 1).toBe(1);
  });

  it("leaves the page's own markup in the body", async () => {
    // The hoist takes the run of head elements the markup opens with and
    // nothing else. A page is still a page.
    const html = await documentFor({ title: "The manual" });

    const root = html.indexOf('<div id="uf-root">');
    expect(root).toBeGreaterThan(html.indexOf("</head>"));
    expect(html.indexOf("a page")).toBeGreaterThan(root);
  });

  it("puts them in the same place as a document the app renders itself", async () => {
    // The check the issue asked for, and the shape of the claim: the two
    // document shapes are supposed to be a choice about who writes `<html>`,
    // not a choice about whether the metadata works. A silent difference
    // between them is the part that was not acceptable.
    const metadata = {
      title: "The manual",
      description: "Everything uf does.",
      canonical: "https://docs.uniflowed.dev/guide",
    };
    const shell = await documentFor(metadata);
    const owned = await documentFor(metadata, undefined, OwnDocument);

    // The two really are the two shapes, so the comparison below means
    // something: one is wrapped in uf's shell and the other is not.
    expect(shell).toContain('id="uf-root"');
    expect(owned).not.toContain('id="uf-root"');
    for (const tag of ["<title>The manual</title>", 'name="description"', 'rel="canonical"']) {
      expect(inHead(shell, tag)).toBe(true);
      expect(inHead(owned, tag)).toBe(true);
    }
  });
});

describe("what was already there", () => {
  it("still renders the five tags that existed before any of this", async () => {
    const html = await documentFor({
      title: "The manual",
      description: "Everything uf does.",
      openGraph: {
        title: "uf",
        description: "A Rust-native, Flow-first React toolchain.",
        images: ["https://docs.uniflowed.dev/og.png"],
      },
    });

    expect(html).toContain("<title>The manual</title>");
    expect(html).toContain('<meta name="description" content="Everything uf does."/>');
    expect(html).toContain('<meta property="og:title" content="uf"/>');
    expect(html).toContain(
      '<meta property="og:description" content="A Rust-native, Flow-first React toolchain."/>',
    );
    expect(html).toContain(
      '<meta property="og:image" content="https://docs.uniflowed.dev/og.png"/>',
    );
  });
});

/**
 * The document a single route renders to, from the modules it is given.
 *
 * `documentFor` is the shorthand over it; a test reaches for this one when it
 * needs a page module of its own rather than a metadata object.
 */
async function documentOf(page: PageModule, layout?: ?LayoutModule): Promise<string> {
  const { prerender } = createRenderer({
    App: routerView("./app"),
    routes: [
      {
        path: "/guide",
        params: [],
        mdx: false,
        file: "app/guide/$page.js",
        page: () => Promise.resolve(page),
        layouts: layout == null ? [] : [() => Promise.resolve(layout)],
      },
    ],
    notFound: [],
    errors: [],
  });

  const result = await prerender("/guide", assets);
  // A throw anywhere above would render the error page instead, and every
  // `not.toContain` below would then pass for the wrong reason.
  expect(result.status).toBe(200);
  return result.html;
}

describe("what a page tells a crawler", () => {
  it("says noindex, nofollow when it wants neither", async () => {
    const html = await documentFor({ title: "a preview", robots: { index: false, follow: false } });

    expect(html).toContain('<meta name="robots" content="noindex, nofollow"/>');
  });

  it("says nothing at all when the page declares nothing", async () => {
    // "index, follow" is what a document with no `robots` meta already means,
    // so a tag that spelled it out would tell a crawler what it had assumed.
    const html = await documentFor({ title: "an ordinary page" });

    expect(html).not.toContain('name="robots"');
  });

  it("keeps a directive that agrees with the default, because it is overruling one", async () => {
    // A section declared `index: false` on its layout and one page under it is
    // public. Dropping `index` for agreeing with the crawler's default would
    // leave that page saying nothing, in a document whose own layout is the
    // reason it had to say something.
    const html = await documentFor({ robots: { index: true } }, { robots: { index: false } });

    expect(html).toContain('<meta name="robots" content="index"/>');
  });

  it("carries the two directives that change what a result looks like", async () => {
    const html = await documentFor({
      title: "a page",
      robots: { maxSnippet: 160, maxImagePreview: "large" },
    });

    expect(html).toContain(
      '<meta name="robots" content="max-snippet:160, max-image-preview:large"/>',
    );
  });
});

describe("the translations of a page", () => {
  it("is one hreflang link each, resolved against metadataBase", async () => {
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      alternates: { languages: { en: "/guide", ja: "/ja/guide" } },
    });

    expect(html).toContain(
      '<link rel="alternate" hrefLang="en" href="https://docs.uniflowed.dev/guide"/>',
    );
    expect(html).toContain(
      '<link rel="alternate" hrefLang="ja" href="https://docs.uniflowed.dev/ja/guide"/>',
    );
  });

  it("takes x-default like any other tag, because to a crawler it is one", async () => {
    const html = await documentFor({
      alternates: { languages: { "x-default": "https://docs.uniflowed.dev/guide" } },
    });

    expect(html).toContain(
      '<link rel="alternate" hrefLang="x-default" href="https://docs.uniflowed.dev/guide"/>',
    );
  });

  it("comes down from the layout, because the set is the same on every page of it", async () => {
    const html = await documentFor(
      { title: "a page" },
      { alternates: { languages: { ja: "/ja" } } },
    );

    expect(html).toContain('<link rel="alternate" hrefLang="ja" href="/ja"/>');
  });

  it("is absent from a page that has no translations", async () => {
    const html = await documentFor({ title: "a page" });

    expect(html).not.toContain('rel="alternate"');
  });
});

describe("a page in a sequence", () => {
  it("links to the pages either side of it", async () => {
    const html = await documentFor({
      metadataBase: "https://docs.uniflowed.dev",
      canonical: "/posts?page=4",
      pagination: { prev: "/posts?page=3", next: "/posts?page=5" },
    });

    expect(html).toContain('<link rel="prev" href="https://docs.uniflowed.dev/posts?page=3"/>');
    expect(html).toContain('<link rel="next" href="https://docs.uniflowed.dev/posts?page=5"/>');
  });

  it("keeps its own canonical URL rather than pointing at page one", async () => {
    // The mistake this pair exists to make unnecessary. A paginated list whose
    // every page is canonical to page one has told a search engine that pages
    // two onwards are duplicates, and everything only linked from them stops
    // being reachable.
    const html = await documentFor({
      canonical: "https://docs.uniflowed.dev/posts?page=4",
      pagination: { prev: "https://docs.uniflowed.dev/posts?page=3" },
    });

    expect(html).toContain(
      '<link rel="canonical" href="https://docs.uniflowed.dev/posts?page=4"/>',
    );
  });

  it("emits only the end it has, on the first page of a list", async () => {
    const html = await documentFor({ pagination: { next: "/posts?page=2" } });

    expect(html).toContain('<link rel="next" href="/posts?page=2"/>');
    expect(html).not.toContain('rel="prev"');
  });
});

describe("structured data", () => {
  it("is one script per entry, in the vocabulary a search engine reads", async () => {
    const html = await documentFor({
      title: "Install · uf",
      jsonLd: [{ "@context": "https://schema.org", "@type": "TechArticle", name: "Install uf" }],
    });

    expect(html).toContain('<script type="application/ld+json">');
    expect(html).toContain('"@type":"TechArticle"');
  });

  it("adds to what the layout declared rather than replacing it", async () => {
    // The one field where the nearest declaration does not win. An
    // `Organization` on the root layout and an `Article` on the page are two
    // statements about one page — replacing would mean a page that describes
    // itself silently deletes the site's description of itself.
    const html = await documentFor(
      { jsonLd: [{ "@type": "TechArticle" }] },
      { jsonLd: [{ "@type": "Organization" }] },
    );

    expect(html).toContain('"@type":"Organization"');
    expect(html).toContain('"@type":"TechArticle"');
  });

  it("cannot be closed from inside its own data", async () => {
    // The escape that matters. A `</script>` in any string in the data would
    // end the element early, and everything after it would be markup the page
    // never wrote — which is the whole of the injection.
    const html = await documentFor({
      jsonLd: [{ "@type": "Article", headline: "</script><img src=x onerror=alert(1)>" }],
    });

    expect(html).not.toContain("<img src=x");
    expect(html).toContain("\\u003c/script");
  });

  it("is absent from a page that declares none", async () => {
    const html = await documentFor({ title: "a page" });

    expect(html).not.toContain("application/ld+json");
  });
});

describe("useSeo", () => {
  it("puts a component's own tags in the document the server writes", async () => {
    // The case `metadata` cannot serve: a pager knows which page of the list
    // it is drawing, and the route module that renders it does not. The tags
    // have to reach the *prerendered* document, because a hook that only
    // worked in a browser would be right on screen and missing from every
    // crawler — which is exactly what `useHead` in `@uniflowed/web` says about
    // itself.
    component Pager() {
      const seo = useSeo({ pagination: { prev: "/posts?page=1", next: "/posts?page=3" } });
      return <nav>{seo}a pager</nav>;
    }

    const html = await documentOf({ default: Pager, metadata: { title: "Posts" } });

    expect(html).toContain('<link rel="prev" href="/posts?page=1"/>');
    expect(html).toContain('<link rel="next" href="/posts?page=3"/>');
  });

  it("resolves its URLs against the metadataBase the layout declared", async () => {
    // The one thing a component three levels down cannot know, and the reason
    // this is a hook rather than a handful of tags written by hand.
    component Deep() {
      const seo = useSeo({ canonical: "/posts?page=2" });
      return <p>{seo}deep</p>;
    }

    const html = await documentOf(
      { default: Deep },
      { default: SiteLayout, metadata: { metadataBase: "https://docs.uniflowed.dev" } },
    );

    expect(html).toContain(
      '<link rel="canonical" href="https://docs.uniflowed.dev/posts?page=2"/>',
    );
  });
});

// ---------------------------------------------------------------------------
// Where a `useSeo` tag lands, which is the same question one layer down
// ---------------------------------------------------------------------------
//
// ubugeeei-prod/uf#547 was "metadata renders into the body on uf's fallback
// shell", and #566 answered it by keeping uf's own `<head>` open until React's
// tags arrive and lifting the leading run of `<title>`/`<meta>`/`<link>` into
// it. ubugeeei-prod/uf#570 asks the same question about `useSeo`, which returns
// elements the caller renders — so the tags are written wherever the caller is,
// which is below that leading run.
//
// They are not, and the reason is worth writing down rather than rediscovering:
// **React does the hoisting first.** A `<title>`, a `<meta>` and a `<link>` are
// hoistable elements, and Fizz emits every one it found while rendering the
// *shell* at the front of its output — before the markup they were written
// inside, at any depth. uf's "leading run" is therefore not "the tags the route
// module rendered": it is every hoistable element in the shell, already
// gathered by React. The shell is complete before React writes a byte, so this
// costs nothing and holds nothing.
//
// What it does not cover is a tag inside a `<Suspense>` boundary that resolves
// *after* the shell — a deferred loader's page. That tag is in a later chunk by
// construction, the head has gone, and React moves it into `document.head` on
// the client where there is one. The routing guide already says the rule that
// follows from it: a route that wants to stream keeps its metadata static.
//
// So the tests below are #570's answer rather than a fix for it: `useSeo` deep
// in a tree, on both document shapes, asserted — because "it happens to work"
// and "it is guaranteed to keep working" are different claims, and only the
// second one survives the next change to `hoisted`.

/** A component three levels down from the page that declares a tag of its own. */
component Pagination() {
  const seo = useSeo({
    canonical: "https://docs.uniflowed.dev/posts?page=2",
    pagination: { prev: "/posts?page=1", next: "/posts?page=3" },
  });
  return <nav className="pager">{seo}page 2</nav>;
}

component DeepPage() {
  return (
    <article>
      <section>
        <Pagination />
      </section>
    </article>
  );
}

describe("a tag a component renders on its way past", () => {
  it("reaches the head from as deep in the tree as it was written", async () => {
    // The shape #570 feared: on the fallback shell, a tag written below the
    // leading run would land inside `<div id="uf-root">`, where a crawler
    // ignores a canonical link. It does not, and nothing here asserted that it
    // does not — which is the whole difference between a behaviour and a
    // guarantee.
    const html = await documentOf({ default: DeepPage, metadata: { title: "Posts" } });

    expect(html).toContain('id="uf-root"');
    expect(inHead(html, '<link rel="canonical"')).toBe(true);
    expect(inHead(html, '<link rel="prev"')).toBe(true);
    expect(inHead(html, '<link rel="next"')).toBe(true);
    // And the component's own markup is still where the component is.
    expect(html.indexOf("page 2")).toBeGreaterThan(html.indexOf("</head>"));
  });

  it("puts them in the same place as a document the app renders itself", async () => {
    // The claim #547 made and #570 extends: the two document shapes are a
    // choice about who writes `<html>`, not about whether the metadata works.
    const shell = await documentOf({ default: DeepPage, metadata: { title: "Posts" } });
    const owned = await documentOf(
      { default: DeepPage, metadata: { title: "Posts" } },
      {
        default: OwnDocument,
      },
    );

    expect(shell).toContain('id="uf-root"');
    expect(owned).not.toContain('id="uf-root"');
    for (const tag of ['rel="canonical"', 'rel="prev"', 'rel="next"']) {
      expect(inHead(shell, tag)).toBe(true);
      expect(inHead(owned, tag)).toBe(true);
    }
  });
});

// ---------------------------------------------------------------------------
// The anchor a uf application has without asking for one
// ---------------------------------------------------------------------------
//
// `RenderProvider` fixes the render's instant, zone and seed, writes them into
// the markup and reads them back on the client, which is what makes
// `useRenderedAt` and `useRandom` agree across hydration. An application that
// did not render one got no error — it got a silent mismatch on every page with
// a clock or a shuffle on it. A guarantee that depends on remembering to opt in
// is not one, so `routerView` renders it. See ubugeeei-prod/uf#559.
//
// It is asserted here rather than in `hooks-ssr.test.js` because what makes it
// possible is where the carrier lands: a `<meta>`, hoisted into the head of
// both document shapes, which is this file's subject.

describe("the render anchor", () => {
  it("is in the document without the application having said anything", async () => {
    const html = await documentOf({ default: Page, metadata: { title: "Posts" } });

    expect(inHead(html, '<meta name="uf:render"')).toBe(true);
    expect(html.split('name="uf:render"').length - 1).toBe(1);
  });

  it("is in the head of an app-owned document too, and only once", async () => {
    // The shape the carrier had to change for: a `<script>` rendered above a
    // root layout that owns `<html>` would have gone *before* the document.
    const owned = await documentOf(
      { default: Page, metadata: { title: "Posts" } },
      {
        default: OwnDocument,
      },
    );

    expect(owned.indexOf("<!doctype html>")).toBe(0);
    expect(inHead(owned, '<meta name="uf:render"')).toBe(true);
    expect(owned.split('name="uf:render"').length - 1).toBe(1);
  });

  it("hands the tree an instant and a seed that a second render reproduces", async () => {
    // The point of the anchor rather than the shape of it: two renders of the
    // same route produce the same numbers, which is what hydration compares.
    component Clock() {
      const at = useRenderedAt();
      const drawn = useRandom("featured").next();
      return <output>{`${at.toString()} ${drawn.toFixed(6)}`}</output>;
    }

    const first = await documentOf({ default: Clock });
    const anchor = first.match(/name="uf:render" content="([^"]*)"/);
    expect(anchor).not.toBe(null);
    const envelope = JSON.parse(
      (anchor?.[1] ?? "").replaceAll("&quot;", '"').replaceAll("&lt;", "<").replaceAll("&gt;", ">"),
    );
    expect(typeof envelope.at).toBe("number");
    expect(typeof envelope.seed).toBe("string");

    // The instant in the markup is the instant the tree was handed.
    expect(first).toContain(new Date(envelope.at).toISOString().replace(".000Z", "Z"));
  });
});
