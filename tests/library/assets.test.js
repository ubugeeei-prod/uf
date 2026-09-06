// @flow
//
// The join between the pipeline and the page.
//
// `crates/uf_assets` writes files and returns a manifest; `uf:asset` turns that
// manifest into what an `import` evaluates to; `Image` and `Font` turn *that*
// into markup. This file is about the two steps in the middle, and it asserts
// on the strings a browser actually receives — the `srcset`, the `sizes`, the
// preload — rather than on the props a component was handed.
//
// The manifests below are shaped exactly as `uf assets` replies; the Rust side
// asserts that the files they name are on disk at the widths they claim
// (`crates/uf_assets/src/image/tests.rs`), and `crates/uf_cli/tests/assets.rs`
// asserts that a real `uf build` emits both halves together. This is the seam
// between those two, which is where a `srcset` that names files nobody wrote
// would otherwise hide.

import { describe, expect, it } from "@uniflowed/testing";
import { render, screen } from "@uniflowed/react-testing";
import { Font, Image } from "@uniflowed/web";

import { assetModuleSource, withUrls } from "../../packages/vite/internal/assets.js";

/** A reply from `uf assets` for a photograph: three JPEG widths, no WebP. */
const photograph = () => ({
  width: 1600,
  height: 1000,
  format: "jpeg",
  variants: [
    {
      file: "hero.aa11bb22-640w.jpg",
      width: 640,
      height: 400,
      format: "jpeg",
      mime: "image/jpeg",
      bytes: 41_000,
    },
    {
      file: "hero.cc33dd44-1600w.jpg",
      width: 1600,
      height: 1000,
      format: "jpeg",
      mime: "image/jpeg",
      bytes: 210_000,
    },
  ],
  blur: "data:image/jpeg;base64,QUJD",
  declined: [{ format: "webp", width: 640, bytes: 900_000, fallbackBytes: 41_000 }],
  note: null,
});

/** A reply for a flat graphic, where the lossless WebP did come out smaller. */
const graphic = () => ({
  width: 512,
  height: 512,
  format: "png",
  variants: [
    {
      file: "mark.ee55ff66-256w.png",
      width: 256,
      height: 256,
      format: "png",
      mime: "image/png",
      bytes: 3_100,
    },
    {
      file: "mark.11772288-256w.webp",
      width: 256,
      height: 256,
      format: "webp",
      mime: "image/webp",
      bytes: 1_900,
    },
    {
      file: "mark.33994400-512w.png",
      width: 512,
      height: 512,
      format: "png",
      mime: "image/png",
      bytes: 8_800,
    },
  ],
  blur: null,
  declined: [],
  note: null,
});

describe("what an import evaluates to", () => {
  it("builds a srcset naming every emitted width", () => {
    const asset = withUrls(photograph(), "/assets/");

    expect(asset.srcSet).toBe(
      "/assets/hero.aa11bb22-640w.jpg 640w, /assets/hero.cc33dd44-1600w.jpg 1600w",
    );
  });

  it("points src at the widest file in the fallback format", () => {
    // The `src` is what a browser with no `srcset` support fetches, and what
    // every browser fetches when `sizes` resolves to the full width.
    const asset = withUrls(photograph(), "/assets/");

    expect(asset.src).toBe("/assets/hero.cc33dd44-1600w.jpg");
    expect(asset.width).toBe(1600);
    expect(asset.height).toBe(1000);
  });

  it("offers a modern format as its own source, and only when there is one", () => {
    const withWebp = withUrls(graphic(), "/assets/");
    const withoutWebp = withUrls(photograph(), "/assets/");

    expect(withWebp.sources.length).toBe(1);
    expect(withWebp.sources[0].type).toBe("image/webp");
    expect(withWebp.sources[0].srcSet).toBe("/assets/mark.11772288-256w.webp 256w");
    // The fallback is never a `<source>`: a browser takes the first one it
    // understands, so offering PNG first would mean the WebP is never used.
    expect(withWebp.srcSet).toContain(".png 256w");
    // And a photograph, where lossless WebP lost on size, gets none at all.
    expect(withoutWebp.sources.length).toBe(0);
  });

  it("is the same manifest in dev and in a build, up to the URL prefix", () => {
    // The property the whole feature rests on. One pipeline, one set of bytes,
    // one set of names; the only thing that differs is where they are served.
    const built = withUrls(graphic(), "/assets/");
    const dev = withUrls(graphic(), "/@uf-asset/");

    expect(dev.srcSet).toBe(built.srcSet.replaceAll("/assets/", "/@uf-asset/"));
    expect(dev.width).toBe(built.width);
    expect(dev.sources[0].srcSet).toBe(
      built.sources[0].srcSet.replaceAll("/assets/", "/@uf-asset/"),
    );
  });

  it("carries the formats it made and threw away", () => {
    // A pipeline that emits nothing modern looks the same as a broken one
    // unless it says what it measured.
    const asset = withUrls(photograph(), "/assets/");

    expect(asset.declined.length).toBe(1);
    expect(asset.declined[0].format).toBe("webp");
    expect(asset.declined[0].bytes > asset.declined[0].fallbackBytes).toBe(true);
  });

  it("emits a module the bundler can inline", () => {
    const source = assetModuleSource(withUrls(graphic(), "/assets/"));

    expect(source.startsWith("export default Object.freeze({")).toBe(true);
    expect(source).toContain("mark.11772288-256w.webp");
  });
});

describe("Image, given an imported image", () => {
  it("writes the srcset the pipeline produced", () => {
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" />);
    const image = screen.getByRole("img");

    expect(image.getAttribute("srcset")).toBe(
      "/assets/hero.aa11bb22-640w.jpg 640w, /assets/hero.cc33dd44-1600w.jpg 1600w",
    );
  });

  it("takes its intrinsic size from the file rather than from a prop", () => {
    // Nobody types the dimensions any more, and nobody gets them wrong.
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" />);
    const image = screen.getByRole("img");

    expect(image.getAttribute("width")).toBe("1600");
    expect(image.getAttribute("height")).toBe("1000");
  });

  it("caps sizes at the intrinsic width instead of letting it mean 100vw", () => {
    // A `srcset` with `w` descriptors and no `sizes` means `100vw`, so the
    // browser fetches a variant wide enough for the whole viewport however
    // small the image is drawn — the ladder gets built and the widest rung
    // taken every time.
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" />);

    expect(screen.getByRole("img").getAttribute("sizes")).toBe("(max-width: 1600px) 100vw, 1600px");
  });

  it("lets a layout that knows better say so", () => {
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" sizes="50vw" />);

    expect(screen.getByRole("img").getAttribute("sizes")).toBe("50vw");
  });

  it("wraps the img in a picture when a smaller format exists", () => {
    const { container } = render(<Image src={withUrls(graphic(), "/assets/")} alt="a" />);
    const sources = container.querySelectorAll("source");

    expect(sources.length).toBe(1);
    expect(sources[0].getAttribute("type")).toBe("image/webp");
    expect(sources[0].getAttribute("srcset")).toBe("/assets/mark.11772288-256w.webp 256w");
    // The `<img>` is still there and is still the fallback: a `<picture>`
    // without one renders nothing at all in the browser that took no source.
    expect(container.querySelector("picture img")).not.toBe(null);
  });

  it("does not wrap anything when there is only the fallback format", () => {
    // A `<picture>` with one `<img>` and no `<source>` is an element that does
    // nothing, on every image in a project full of photographs.
    const { container } = render(<Image src={withUrls(photograph(), "/assets/")} alt="a" />);

    expect(container.querySelector("picture")).toBe(null);
    expect(container.querySelector("img")).not.toBe(null);
  });

  it("shows the blur placeholder until the real bytes land", () => {
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" />);

    expect(screen.getByRole("img").style.backgroundImage).toContain("data:image/jpeg;base64,QUJD");
  });

  it("can be told not to", () => {
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" placeholder={false} />);

    expect(screen.getByRole("img").style.backgroundImage).toBe("");
  });

  it("refuses to render an image whose size it does not know", () => {
    // The mistake this component exists to prevent. An SVG comes back from the
    // pipeline with no intrinsic size — correctly, it has none — and an `<img>`
    // with no ratio moves every line below it when it lands.
    const svg = withUrls(
      {
        width: null,
        height: null,
        format: "original",
        variants: [
          {
            file: "m.44556677.svg",
            width: 0,
            height: 0,
            format: "original",
            mime: "image/svg+xml",
            bytes: 90,
          },
        ],
        blur: null,
        declined: [],
        note: "an SVG is resolution independent",
      },
      "/assets/",
    );

    expect(() => render(<Image src={svg} alt="a" />)).toThrow();
    // And it says why, so the fix is obvious.
    try {
      render(<Image src={svg} alt="a" />);
    } catch (error) {
      expect(String(error)).toContain("resolution independent");
    }
  });

  it("renders that image once the page supplies the size the file could not", () => {
    const svg = withUrls(
      {
        width: null,
        height: null,
        format: "original",
        variants: [
          {
            file: "m.44556677.svg",
            width: 0,
            height: 0,
            format: "original",
            mime: "image/svg+xml",
            bytes: 90,
          },
        ],
        blur: null,
        declined: [],
        note: "an SVG is resolution independent",
      },
      "/assets/",
    );
    render(<Image src={svg} alt="a" width={24} height={24} />);

    expect(screen.getByRole("img").getAttribute("src")).toBe("/assets/m.44556677.svg");
    expect(screen.getByRole("img").getAttribute("width")).toBe("24");
  });
});

describe("Image, above the fold", () => {
  /** The preload React hoisted into `<head>` for one href. */
  function preloadFor(href: string): Element | null {
    return globalThis.document.head.querySelector(`link[rel="preload"][href="${href}"]`);
  }

  it("preloads the one image the page is judged on", () => {
    // A `srcSet` behind a lazy `<img>` is discovered late, and for the largest
    // image on the page that is most of the Largest Contentful Paint.
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" priority={true} />);
    const link = preloadFor("/assets/hero.cc33dd44-1600w.jpg");

    expect(link?.getAttribute("as")).toBe("image");
    expect(link?.getAttribute("fetchpriority")).toBe("high");
  });

  it("preloads the same variant the img will ask for", () => {
    // If the two disagree the browser fetches both, which is the opposite of
    // what a preload is for.
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" priority={true} />);
    const link = preloadFor("/assets/hero.cc33dd44-1600w.jpg");
    const image = screen.getByRole("img");

    expect(link?.getAttribute("imagesrcset")).toBe(image.getAttribute("srcset"));
    expect(link?.getAttribute("imagesizes")).toBe(image.getAttribute("sizes"));
  });

  it("is eager, because a preloaded lazy image is a contradiction", () => {
    render(<Image src={withUrls(photograph(), "/assets/")} alt="a" priority={true} />);

    expect(screen.getByRole("img").getAttribute("loading")).toBe("eager");
    expect(screen.getByRole("img").getAttribute("fetchpriority")).toBe("high");
  });

  it("preloads nothing for the images below it", () => {
    render(<Image src={withUrls(graphic(), "/assets/")} alt="a" />);

    expect(preloadFor("/assets/mark.33994400-512w.png")).toBe(null);
    expect(screen.getByRole("img").getAttribute("loading")).toBe("lazy");
    expect(screen.getByRole("img").getAttribute("fetchpriority")).toBe(null);
  });
});

describe("Font, given an imported font", () => {
  /**
   * The stylesheet React hoisted into `<head>` for one font.
   *
   * `data-href`, not `href`: React takes `href` as the key it dedupes hoisted
   * styles by and does not render it as an attribute, so a query for
   * `style[href]` finds nothing and finds it silently.
   */
  function sheetFor(href: string): Element | null {
    return globalThis.document.head.querySelector(`style[data-href="${href}"]`);
  }

  const inter = () => ({
    src: "/assets/Inter.99aabbcc.woff2",
    family: "Inter",
    fallbackFamily: "Inter Fallback",
    fontFamily: '"Inter", "Inter Fallback", "Arial"',
    type: "font/woff2",
    css:
      '@font-face{font-family:"Inter";font-style:normal;font-weight:400;font-display:swap;' +
      'src:url("/assets/Inter.99aabbcc.woff2") format("woff2");}' +
      '@font-face{font-family:"Inter Fallback";font-style:normal;font-weight:400;' +
      'src:local("Arial");size-adjust:107.4%;ascent-override:90.2%;' +
      "descent-override:22.48%;line-gap-override:0%;}",
  });

  it("declares the face it just preloaded", () => {
    // The preload alone never made the text render in the font: it moved the
    // fetch earlier and left the declaring to a stylesheet uf did not write.
    const { container } = render(<Font src={inter()} />);
    const style = sheetFor("/assets/Inter.99aabbcc.woff2");

    expect(style?.textContent).toContain("@font-face");
    expect(style?.textContent).toContain('src:url("/assets/Inter.99aabbcc.woff2")');
    // Hoisted, like the preload: written beside the text that needs it.
    expect(container.querySelector("style")).toBe(null);
  });

  it("emits the metric-matched fallback that stops the swap moving the page", () => {
    render(<Font src={inter()} />);
    const css = sheetFor("/assets/Inter.99aabbcc.woff2")?.textContent ?? "";

    expect(css).toContain('font-family:"Inter Fallback"');
    expect(css).toContain('src:local("Arial")');
    for (const property of [
      "size-adjust:",
      "ascent-override:",
      "descent-override:",
      "line-gap-override:",
    ]) {
      expect(css).toContain(property);
    }
  });

  it("still preloads, with the crossOrigin that stops the second request", () => {
    render(<Font src={inter()} />);
    const link = globalThis.document.head.querySelector(
      'link[rel="preload"][href="/assets/Inter.99aabbcc.woff2"]',
    );

    expect(link?.getAttribute("as")).toBe("font");
    expect(link?.getAttribute("crossorigin")).toBe("anonymous");
    expect(link?.getAttribute("type")).toBe("font/woff2");
  });

  it("names the whole stack, since the fallback only applies when it is asked for", () => {
    // A page that sets `font-family: Inter` gets the download and none of the
    // metric matching, which is the failure this field exists to prevent.
    expect(inter().fontFamily).toBe('"Inter", "Inter Fallback", "Arial"');
  });
});
