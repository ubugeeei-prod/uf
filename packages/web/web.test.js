// @flow
//
// The primitives a page is built from.
//
// Each of these exists to prevent one specific mistake, so each test is about
// that mistake rather than about the element rendering at all: a font preload
// that downloads twice, a date that makes the server and the browser disagree,
// a live region that is never announced, a cookie parser a `__proto__` can
// poison.

import { createRequire } from "node:module";

import type * as React from "@uniflowed/react";
import { describe, expect, it } from "@uniflowed/testing";
import { RenderProvider } from "@uniflowed/hooks";
import { fireEvent, render, screen } from "@uniflowed/react-testing";
import {
  Announcer,
  Font,
  Image,
  Page,
  Picture,
  SkipLink,
  Time,
  asInstant,
  relative,
} from "@uniflowed/web";

// Loaded the way `hooks-ssr.test.js` loads it: through a synchronous require, so
// that importing this file costs nothing until a test asks for server markup.
// `renderToStaticMarkup` is what a prerender writes, and it reads no document —
// which is the point of the `Time, in a prerender` block below.
const server = createRequire(import.meta.url)("react-dom/server");

const markupOf = (element: React.Node): string => String(server.renderToStaticMarkup(element));

/**
 * The preload React hoisted into `<head>` for one href.
 *
 * `<link rel="preload">` never appears in the render container: React lifts it
 * to the document itself, which is the whole reason `Font` can be written
 * beside the text that needs the font. So the assertion has to look where the
 * element actually went.
 */
function preloadFor(href: string): Element | null {
  return globalThis.document.head.querySelector(`link[rel="preload"][href="${href}"]`);
}

describe("Image", () => {
  it("carries the dimensions that reserve the space", () => {
    // A browser that knows the intrinsic ratio reserves the box before the
    // bytes arrive; one that does not reflows everything below.
    render(<Image src="/a.png" alt="a" width={320} height={240} />);
    const image = screen.getByRole("img");

    expect(image.getAttribute("width")).toBe("320");
    expect(image.getAttribute("height")).toBe("240");
  });

  it("is lazy and decodes off the main thread by default", () => {
    render(<Image src="/a.png" alt="a" width={1} height={1} />);
    const image = screen.getByRole("img");

    expect(image.getAttribute("loading")).toBe("lazy");
    expect(image.getAttribute("decoding")).toBe("async");
  });

  it("lets the one image above the fold say so", () => {
    render(<Image src="/hero.png" alt="hero" width={1} height={1} loading="eager" />);

    expect(screen.getByRole("img").getAttribute("loading")).toBe("eager");
  });
});

describe("Picture", () => {
  it("keeps the img, which is what renders when no source matched", () => {
    // A `<picture>` with no `<img>` renders nothing at all, and only in the
    // browser that took none of the sources.
    render(
      <Picture
        src="/a.png"
        alt="a"
        width={2}
        height={2}
        sources={[{ srcSet: "/a.avif", type: "image/avif" }]}
      />,
    );

    expect(screen.getByRole("img").getAttribute("src")).toBe("/a.png");
  });
});

describe("Font", () => {
  it("always sets crossOrigin, or the preload is a second request", () => {
    // A font is fetched in CORS mode whatever its origin, so a preload without
    // this downloads the file twice and the text still arrives late. It fails
    // silently, which is why it is not optional.
    render(<Font src="/inter.woff2" />);
    const link = preloadFor("/inter.woff2");

    expect(link?.getAttribute("crossorigin")).toBe("anonymous");
    expect(link?.getAttribute("rel")).toBe("preload");
    expect(link?.getAttribute("as")).toBe("font");
  });

  it("reaches the head from wherever it is written", () => {
    // React hoists a `<link rel="preload">` into `<head>` itself, which is why
    // `Font` is an ordinary component rather than something a page has to lift
    // to its root: the component that needs the font is the one that asks for
    // it, and the request still starts at the top of the document.
    const { container } = render(
      <main>
        <p>
          <Font src="/deep.woff2" />
        </p>
      </main>,
    );

    expect(container.querySelector("link")).toBe(null);
    expect(preloadFor("/deep.woff2")).not.toBe(null);
  });
});

describe("Time", () => {
  it("puts the exact instant in dateTime, whatever the text says", () => {
    render(<Time value="2026-09-04T06:00:00Z" />);

    const element = screen.getByText("2026-09-04T06:00:00Z");
    expect(element.getAttribute("datetime")).toBe("2026-09-04T06:00:00Z");
  });

  it("renders the same string a server would, so hydration matches", () => {
    // `toLocaleString` on a server in UTC and a browser in Tokyo disagree, and
    // React notices.
    render(<Time value="2026-09-04T06:00:00Z" format="date" />);

    expect(screen.getByText("2026-09-04")).toBeInTheDocument();
  });

  it("writes the calendar date of the zone it was given, not of UTC", () => {
    // 22:00 UTC is the following morning in Tokyo, and a date component that
    // took the day from the instant would print yesterday for every reader east
    // of the meridian.
    render(<Time value="2026-09-04T22:00:00Z" format="date" zone="Asia/Tokyo" />);

    expect(screen.getByText("2026-09-05")).toBeInTheDocument();
  });

  it("says which zone the wall clock it prints belongs to", () => {
    // The zoned form is the one a reader can act on before hydration: an
    // absolute instant they can read, with the offset that makes it unambiguous.
    render(<Time value="2026-09-04T06:00:00Z" format="zoned" zone="Asia/Tokyo" />);

    expect(screen.getByText("2026-09-04 15:00 +09:00")).toBeInTheDocument();
  });

  it("falls back to UTC rather than to the machine it is running on", () => {
    // The default has to be a zone both renders agree on. The host's zone is the
    // one thing that is guaranteed to differ between a server and a reader,
    // which is the whole problem restated.
    render(<Time value="2026-09-04T22:00:00Z" format="date" />);

    expect(screen.getByText("2026-09-04")).toBeInTheDocument();
  });

  it("accepts an instant as a Date, a string, or a number", () => {
    const at = new Date("2026-01-01T00:00:00.000Z");
    const { container } = render(
      <>
        <Time value={at} />
        <Time value={at.toISOString()} />
        <Time value={at.getTime()} />
      </>,
    );

    const times = container.querySelectorAll("time");
    expect(times.length).toBe(3);
    for (const time of times) {
      expect(time.getAttribute("datetime")).toBe("2026-01-01T00:00:00Z");
    }
  });

  it("refuses a date with no time, which is not an instant", () => {
    // `new Date("2026-09-04")` guesses a midnight, and which midnight has
    // changed between browsers. Temporal refuses the question and so does this.
    expect(() => asInstant("2026-09-04")).toThrow();
  });
});

describe("Time, in a prerender", () => {
  it("puts the deterministic text in the markup a server sends", () => {
    // The claim the whole module exists to keep: what a server writes is a
    // function of the instant and a zone name, so a browser in any zone
    // reproduces it exactly and React finds nothing to complain about. Rendered
    // through `react-dom/server` rather than into a document, because a server
    // has no document and this is the markup that goes on the wire.
    const markup = markupOf(<Time value="2026-09-04T06:00:00Z" format="local" zone="Asia/Tokyo" />);

    // The attribute's spelling is React's — HTML attribute names are
    // case-insensitive, and the DOM test above is what checks a browser reads it
    // — so what is asserted here is the text and the instant beside it.
    expect(markup).toContain(">2026-09-04 15:00 +09:00</time>");
    expect(markup).toContain("2026-09-04T06:00:00Z");
  });

  it("takes its zone from the render, so the browser can reproduce it", () => {
    // Without a `zone` prop the component asks the render what zone it was made
    // in. That answer travels in the markup — the `<meta name="uf:render">`
    // below is `RenderProvider`'s — which is what lets the browser render the
    // same string before it knows anything about the server. 22:00 UTC is
    // already the 5th in Tokyo, so the date is the assertion that the zone was
    // actually used.
    //
    // The envelope is an attribute value, so React escapes the quotes that
    // `JSON.stringify` wrote and the parser gives them back: `&quot;` here is
    // the same two fields, in the carrier's own spelling.
    const markup = markupOf(
      <RenderProvider at={Date.UTC(2026, 8, 4, 22, 0, 0)} timeZone="Asia/Tokyo" seed="fixedseed">
        <Time value="2026-09-04T22:00:00Z" format="date" />
      </RenderProvider>,
    );

    expect(markup).toContain(">2026-09-05<");
    expect(markup).toContain("&quot;timeZone&quot;:&quot;Asia/Tokyo&quot;");
    expect(markup).toContain("&quot;seed&quot;:&quot;fixedseed&quot;");
  });

  it("renders the same bytes twice from the same anchor", () => {
    // Two renders of the same tree, which is what a prerender and the hydration
    // after it are. Nothing here reads a clock, so nothing here can differ.
    const page = (
      <RenderProvider at={Date.UTC(2026, 8, 4, 6, 0, 0)} timeZone="UTC" seed="fixedseed">
        <Time value="2026-09-04T06:00:00Z" format="relative" />
      </RenderProvider>
    );

    expect(markupOf(page)).toBe(markupOf(page));
  });
});

describe("relative", () => {
  it("describes an instant against another", () => {
    const from = new Date("2026-09-04T12:00:00.000Z");

    expect(relative(new Date("2026-09-04T11:57:00.000Z"), from)).toBe("3 minutes ago");
    expect(relative(new Date("2026-09-01T12:00:00.000Z"), from)).toBe("3 days ago");
    expect(relative(new Date("2026-09-04T12:05:00.000Z"), from)).toBe("in 5 minutes");
  });

  it("says yesterday rather than one day ago", () => {
    // `numeric: "auto"` is the reason: the word is what a reader would say,
    // and it is the locale's own — a language with no single word for it gets
    // the counted form back automatically.
    const from = new Date("2026-09-04T12:00:00.000Z");

    expect(relative(new Date("2026-09-03T12:00:00.000Z"), from)).toBe("yesterday");
  });

  it("calls the present moment now rather than in zero seconds", () => {
    const at = new Date("2026-09-04T12:00:00.000Z");

    expect(relative(at, at)).not.toContain("0 second");
  });
});

describe("Announcer", () => {
  it("is a polite live region by default", () => {
    // `assertive` interrupts the reader mid-sentence, which is why "saved" is
    // not announced that way.
    const { container } = render(<Announcer>saved</Announcer>);
    const region = container.querySelector("[aria-live]");

    expect(region?.getAttribute("aria-live")).toBe("polite");
    expect(region?.getAttribute("aria-atomic")).toBe("true");
  });

  it("renders even with nothing in it", () => {
    // A live region that appears at the same moment as its text is not
    // announced — the reader has to be watching it before it changes.
    const { container } = render(<Announcer />);

    expect(container.querySelector("[aria-live]")).not.toBe(null);
  });

  it("can interrupt when something really is urgent", () => {
    const { container } = render(<Announcer urgency="assertive">error</Announcer>);

    expect(container.querySelector("[aria-live]")?.getAttribute("aria-live")).toBe("assertive");
  });
});

describe("Page", () => {
  it("is the landmark skip-to-content skips to", () => {
    render(<Page>content</Page>);

    expect(screen.getByRole("main")).toBeInTheDocument();
  });
});

describe("SkipLink", () => {
  it("points at the landmark Page renders", () => {
    // The two have to agree, and when they do not the link fails silently:
    // focus stays where it was and only a keyboard user ever finds out.
    const { container } = render(
      <>
        <SkipLink />
        <Page>content</Page>
      </>,
    );

    const target = container.querySelector("a")?.getAttribute("href")?.slice(1);
    expect(target).not.toBe(undefined);
    expect(screen.getByRole("main").getAttribute("id")).toBe(target);
  });

  it("is hidden until it is focused, and readable after", () => {
    render(<SkipLink />);
    const link = screen.getByRole("link");

    // Hidden from eyes, not from a screen reader: still in the tree, still
    // reachable by tab.
    expect(link.style.position).toBe("absolute");
    expect(link.style.width).toBe("1px");

    fireEvent.focus(link);

    expect(link.style.width).toBe("");
    expect(link.style.padding).toBe("0.5rem 0.75rem");
  });

  it("says what it does, unless the page says it better", () => {
    const { container } = render(<SkipLink>本文へスキップ</SkipLink>);

    expect(container.textContent).toBe("本文へスキップ");
  });
});
