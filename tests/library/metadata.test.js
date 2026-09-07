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
import { routerView } from "@uniflowed/router";
import type { Metadata } from "@uniflowed/router";
import { createRenderer } from "@uniflowed/router/server";
import { describe, expect, it } from "@uniflowed/test";

const assets = { scripts: [], styles: [], preloads: [] };

component Page() {
  return <p>a page</p>;
}

component SiteLayout(children: React.Node) {
  return <div className="site">{children}</div>;
}

/** The document a single route with this metadata renders to. */
async function documentFor(metadata: Metadata, layoutMetadata?: Metadata): Promise<string> {
  const { prerender } = createRenderer({
    App: routerView("./app"),
    routes: [
      {
        path: "/guide",
        params: [],
        mdx: false,
        file: "app/guide/_uf.page.js",
        page: () => Promise.resolve({ default: Page, metadata }),
        layouts:
          layoutMetadata == null
            ? []
            : [() => Promise.resolve({ default: SiteLayout, metadata: layoutMetadata })],
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
