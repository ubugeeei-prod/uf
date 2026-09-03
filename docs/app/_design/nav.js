// @flow
//
// The manual's table of contents.
//
// One list, in reading order, is the whole navigation model: the sidebar
// renders it, "next page" reads it, and the masthead highlights the section a
// page belongs to. Keeping it in one place means a new page cannot appear in
// the sidebar and be missing from the footer, or the reverse.

/** A page in the manual. `href` is the route, not a file path. */
export type Entry = {|
  +href: string,
  +title: string,
  /** One line, shown when a reader is deciding whether to open the page. */
  +blurb: string,
|};

/** A run of pages under one heading. */
export type Section = {|
  +title: string,
  +pages: $ReadOnlyArray<Entry>,
|};

export const sections: $ReadOnlyArray<Section> = [
  {
    title: "Getting started",
    pages: [
      {
        href: "/guide",
        title: "What uf is",
        blurb: "The argument for one toolchain, and what it costs you.",
      },
      {
        href: "/guide/install",
        title: "Install",
        blurb: "One binary, three runtimes, no plugins to add.",
      },
      {
        href: "/guide/project",
        title: "Your first project",
        blurb: "From an empty directory to a built site.",
      },
    ],
  },
  {
    title: "Writing code",
    pages: [
      {
        href: "/guide/flow",
        title: "Flow, the modern parts",
        blurb: "component, hook, renders, match and enums — and what uf does with them.",
      },
      {
        href: "/guide/routing",
        title: "Routing",
        blurb: "Files become routes; layouts nest; loaders run before the page.",
      },
      {
        href: "/guide/styling",
        title: "Styling and content",
        blurb: "CSS, Markdown and MDX, all on by default.",
      },
    ],
  },
  {
    title: "The tools",
    pages: [
      {
        href: "/guide/dev",
        title: "Dev and build",
        blurb: "Vite runs both; uf decides what it is handed.",
      },
      {
        href: "/guide/testing",
        title: "Testing",
        blurb: "A Rust runner, host workers, and where it stands against Bun.",
      },
      {
        href: "/guide/format",
        title: "Formatting and linting",
        blurb: "The official Flow parser, a Rust printer, and Flow's own lints.",
      },
    ],
  },
  {
    title: "Reference",
    pages: [
      {
        href: "/reference/cli",
        title: "Commands",
        blurb: "Every subcommand, flag and exit code.",
      },
      {
        href: "/reference/config",
        title: "uf.config.js",
        blurb: "Every option, its default, and what reads it.",
      },
      {
        href: "/reference/packages",
        title: "Packages",
        blurb: "What each @uniflowed/* package is for.",
      },
    ],
  },
];

/** Every page, flattened into reading order. */
export const pages: $ReadOnlyArray<Entry> = sections.flatMap(
  (section) => section.pages,
);

/**
 * The entry for a pathname, or `null` for a page outside the manual (the home
 * page, a 404). Trailing slashes are ignored so `/guide/` and `/guide` are the
 * same page.
 */
export function entryFor(pathname: string): Entry | null {
  const normalized = normalize(pathname);
  for (const page of pages) {
    if (page.href === normalized) {
      return page;
    }
  }
  return null;
}

/**
 * The page a reader should go to next, or `null` at the end of the manual.
 * Reading order is the order of `sections`, which is the order the sidebar
 * shows — so "next" always means "the link below this one".
 */
export function nextAfter(pathname: string): Entry | null {
  const normalized = normalize(pathname);
  const index = pages.findIndex((page) => page.href === normalized);
  if (index < 0 || index + 1 >= pages.length) {
    return null;
  }
  return pages[index + 1];
}

/** `true` when `pathname` is the entry's page. */
export function isCurrent(pathname: string, href: string): boolean {
  return normalize(pathname) === href;
}

function normalize(pathname: string): string {
  if (pathname.length > 1 && pathname.endsWith("/")) {
    return pathname.slice(0, -1);
  }
  return pathname;
}
