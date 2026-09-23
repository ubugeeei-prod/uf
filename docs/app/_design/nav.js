// @flow
//
// The manual's table of contents.
//
// The manual has several kinds of reader, and they came for different things:
// to try uf, to decide whether a team should use it, to move an application
// onto it, to build with it, to run it, or to look one thing up. So it is
// sections by reader rather than one sequence. Each section opens with a
// landing page that says who it is for and the order to read it in, and each
// names the landing page its reader goes to when they reach its end.
//
// It is still one list, and the list is still the whole navigation model: the
// sidebar renders it, "next page" reads it, the masthead highlights the part
// of the site a page belongs to, the home page offers its sections as paths,
// and every landing page lists its section's pages from it. Keeping it in one
// place means a new page cannot appear in the sidebar and be missing from its
// landing page, or the reverse.
//
// A page's section is where it is listed, not where it lives: `href` is the
// route, and a page changes section without its URL changing.

/** A page in the manual. `href` is the route, not a file path. */
export type Entry = {|
  readonly href: string,
  readonly title: string,
  /** One line, shown when a reader is deciding whether to open the page. */
  readonly blurb: string,
|};

/** The pages one kind of reader needs, and where that reader goes after. */
export type Section = {|
  readonly title: string,
  /** The question the reader this section is for arrived with. */
  readonly question: string,
  /**
   * The page that says who the section is for, what is in it and the order
   * to read it in. The sidebar's section heading links to it.
   */
  readonly landing: Entry,
  /** The section's pages, in the order its reader should read them. */
  readonly pages: $ReadOnlyArray<Entry>,
  /**
   * The landing page of the section this reader goes to next, or `null` for
   * the section that ends the manual. "Next page" on a section's last page
   * points here rather than at whichever section happens to be listed below:
   * somebody who has finished deciding wants to start, not to read the
   * section that follows in the sidebar.
   */
  readonly then: string | null,
|};

export const sections: $ReadOnlyArray<Section> = [
  {
    title: "Start",
    question: "How do I get it running?",
    landing: {
      href: "/guide/start",
      title: "Start",
      blurb: "Install uf, create a project and build your first app.",
    },
    pages: [
      {
        href: "/guide/install",
        title: "Install",
        blurb: "Install uf on macOS, Linux or Windows.",
      },
      {
        href: "/guide/project",
        title: "Your first project",
        blurb: "Create, run, test and build a React application.",
      },
      {
        href: "/guide/editors",
        title: "Editors",
        blurb:
          "uf lsp in VS Code, Cursor, Neovim, Vim, Helix, Emacs, Zed and JetBrains: what it answers, what it does not, and the config it reads.",
      },
      {
        href: "/guide/tutorial",
        title: "Build a reading list",
        blurb: "Build a React application with routes, shared state, a form and tests.",
      },
    ],
    then: "/guide/build-an-app",
  },
  {
    title: "Why uf",
    question: "Should my team use this, and what does it cost?",
    landing: {
      href: "/guide/why-uf",
      title: "Why uf",
      blurb:
        "For deciding whether to use uf: what it is, what Flow buys, what it costs, and what it will not do.",
    },
    pages: [
      {
        href: "/guide",
        title: "What uf is",
        blurb: "The argument for one toolchain, and what it costs you.",
      },
      {
        href: "/guide/why",
        title: "Why Flow",
        blurb: "What Flow says about React that nothing else can, and what it costs.",
      },
      {
        href: "/guide/typescript",
        title: "Flow and TypeScript",
        blurb: "The rows TypeScript wins, the four Flow wins, and why uf is possible.",
      },
      {
        href: "/guide/compare",
        title: "uf compared",
        blurb: "Against Next.js, Vite, Bun and CRA, including the rows uf loses.",
      },
      {
        href: "/guide/nextjs",
        title: "uf and Next.js",
        blurb:
          "The Next.js App Router, feature by feature: what uf implements, what it does differently, and what is still missing.",
      },
      {
        href: "/guide/vite-plus",
        title: "uf and Vite+",
        blurb: "Where uf sits next to the toolchain it will be compared to.",
      },
      {
        href: "/guide/benchmarks",
        title: "Benchmarks",
        blurb:
          "Every uf command timed beside Vite+, Next.js, Bun, Vitest, ESLint, Prettier, Biome, Flow and pnpm, with the machine and the versions.",
      },
      {
        href: "/guide/scope",
        title: "What uf does not do",
        blurb: "The refusals, and the gaps — the second list with issue numbers.",
      },
      {
        href: "/guide/architecture",
        title: "Architecture",
        blurb: "What went wrong with create-react-app, and the lines uf will not cross.",
      },
    ],
    then: "/guide/start",
  },
  {
    title: "Build an app",
    question: "How do I route, load data, render, style and test?",
    landing: {
      href: "/guide/build-an-app",
      title: "Build an app",
      blurb:
        "One guide per thing an application does: routes, data, rendering, state, forms, UI, styling, and what production asks for.",
    },
    pages: [
      {
        href: "/guide/flow",
        title: "Flow, the modern parts",
        blurb: "component, hook, renders, match and enums — and what uf does with them.",
      },
      {
        href: "/guide/routing",
        title: "Routing",
        blurb: "Files become routes, layouts nest, and Server Components fetch their own data.",
      },
      {
        href: "/guide/routing/requests",
        title: "Answering requests",
        blurb:
          "Middleware, route handlers, draft mode, QUERY, and streams, sockets and work that outlives the request.",
      },
      {
        href: "/guide/rendering",
        title: "Rendering modes",
        blurb: "Where a document comes from, and what the browser does once it has one.",
      },
      {
        href: "/guide/server-components",
        title: "Server Components",
        blurb:
          "The boundary a directive draws, the graph that resolves it, and a split at the route, not the module.",
      },
      {
        href: "/guide/async-react",
        title: "Async React",
        blurb:
          "Suspense, transitions and optimistic UI: what waits, what stays visible, and what is only temporary.",
      },
      {
        href: "/guide/server-actions",
        title: "Server actions",
        blurb:
          "A function the browser calls by id: what may cross, what is refused, and why it authorizes itself.",
      },
      {
        href: "/guide/state",
        title: "State",
        blurb: "Atoms, a store, and where this parts company with Jotai.",
      },
      {
        href: "/guide/form",
        title: "Forms",
        blurb: "Uncontrolled inputs, narrow subscriptions, and no Proxy.",
      },
      {
        href: "/guide/effect",
        title: "Effects",
        blurb: "Failures in the type, and what Flow costs against Effect-TS.",
      },
      {
        href: "/guide/ui",
        title: "Headless components",
        blurb:
          "Headless primitives: keyboard maps, ARIA contracts, composition types, and no styles at all.",
      },
      {
        href: "/guide/styling",
        title: "Styling and content",
        blurb: "CSS, StyleX, tokens, dark mode, Markdown and MDX.",
      },
      {
        href: "/guide/assets",
        title: "Images, fonts, icons and cards",
        blurb:
          "Resized, self-hosted, subsetted and drawn at build time, remote images on request — and what that stops short of.",
      },
      {
        href: "/guide/cache",
        title: "Caching",
        blurb: "Route, function, and fetch caches with explicit lifetimes and pluggable storage.",
      },
      {
        href: "/guide/auth",
        title: "Signing in",
        blurb:
          "A contract rather than a provider: PKCE, a single-use state, and a session store you can replace.",
      },
      {
        href: "/guide/graphql-relay",
        title: "GraphQL and Relay",
        blurb:
          "Relay artifacts, typed components, request-scoped RSC preloads, and an independent backend.",
      },
      {
        href: "/guide/logging",
        title: "Logging",
        blurb: "A structured logger, and a request id readable from inside a render.",
      },
      {
        href: "/guide/vitals",
        title: "Web vitals",
        blurb:
          "Five numbers the browser already has, and nothing that leaves the machine unless you ask.",
      },
    ],
    then: "/guide/targets",
  },
  {
    title: "Targets",
    question: "Where does it run: a server, a static host, a phone, a terminal?",
    landing: {
      href: "/guide/targets",
      title: "Targets",
      blurb:
        "Where a uf application runs: a server or a static host, a single file, a phone, a terminal.",
    },
    pages: [
      {
        href: "/guide/deploy",
        title: "Deploy a web build",
        blurb:
          "Choose how to serve a uf web build: preview, start, adapters, static files, or one executable.",
      },
      {
        href: "/guide/react-native",
        title: "React Native target",
        blurb: "Native route files, Metro, navigator events and the test-tree surface.",
      },
      {
        href: "/guide/tui",
        title: "Terminal UI",
        blurb: "React with a terminal for a host: flexbox, cells, and only the ones that changed.",
      },
    ],
    then: "/guide/toolchain",
  },
  {
    title: "The toolchain",
    question: "How do I pin runtimes, run CI, tune lint and cache tasks?",
    landing: {
      href: "/guide/toolchain",
      title: "The toolchain",
      blurb:
        "One guide per command: what it runs, what it reads, what it refuses, and how to put it in CI.",
    },
    pages: [
      {
        href: "/guide/dev",
        title: "Dev and build",
        blurb: "Vite runs both; uf decides what it is handed.",
      },
      {
        href: "/guide/format",
        title: "Formatting and linting",
        blurb: "The official Flow parser, a Rust printer, and Flow's own lints.",
      },
      {
        href: "/guide/check",
        title: "Type checking",
        blurb:
          "uf check: uf lint, then Flow's own inference over the project and the packages that ship Flow.",
      },
      {
        href: "/guide/testing",
        title: "Testing",
        blurb: "A Rust runner, host workers, and where it stands against Bun.",
      },
      {
        href: "/guide/dependencies",
        title: "Dependencies",
        blurb:
          "uf install and the rest: your project's own package manager, with install scripts refused and CI held to the lockfile.",
      },
      {
        href: "/guide/env",
        title: "Environments",
        blurb: "A pinned toolchain per repository, in a shared store, with a collector.",
      },
      {
        href: "/guide/tasks",
        title: "Tasks",
        blurb: "Define commands, connect dependencies and cache task results.",
      },
      {
        href: "/guide/ci",
        title: "uf in CI",
        blurb:
          "GitHub Actions, GitLab and CircleCI: one step each, why they all pin, and the install a check is worthless without.",
      },
      {
        href: "/guide/agents",
        title: "Agents",
        blurb:
          "uf mcp: twelve tools over stdio, the two that write, and what an agent should not assume.",
      },
    ],
    then: "/reference",
  },
  {
    title: "Migrate",
    question: "How do I move an application I already have?",
    // One page, so it is its own landing page: it opens with where each kind of
    // application is covered, and a second page in front of it would say only
    // "read the next page".
    landing: {
      href: "/guide/migrate",
      title: "Migrating to uf",
      blurb:
        "From CRA, Vite or Next.js: what carries over, the moves in order, and what still differs.",
    },
    pages: [],
    then: "/guide/start",
  },
  {
    title: "Reference",
    question: "What does this flag, key or export do?",
    landing: {
      href: "/reference",
      title: "Reference",
      blurb: "Look up commands, configuration, packages and APIs.",
    },
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
      {
        href: "/reference/ui",
        title: "Components",
        blurb: "The headless primitives, their parts, and the keys each one owns.",
      },
      {
        href: "/reference/hooks",
        title: "Browser hooks",
        blurb:
          "What each one renders before hydration, and how the ones with no server answer say so.",
      },
      {
        href: "/reference/effect",
        title: "Effects",
        blurb: "Typed failures, fibers that own what they start, and what Flow cannot say.",
      },
      {
        href: "/reference/i18n",
        title: "Internationalisation",
        blurb:
          "MessageFormat 2 with typed arguments, the subset uf implements, and where the type system stops.",
      },
      {
        href: "/reference/std",
        title: "Standard library",
        blurb:
          "The Go standard library modules JavaScript does not have, and the measurements behind why they are JavaScript.",
      },
    ],
    then: null,
  },
];

/** Every page, flattened: each section's landing page, then its pages. */
export const pages: $ReadOnlyArray<Entry> = sections.flatMap((section) => [
  section.landing,
  ...section.pages,
]);

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

/** The section a pathname is listed in, or `null` for a page outside the manual. */
export function sectionFor(pathname: string): Section | null {
  const normalized = normalize(pathname);
  for (const section of sections) {
    if (section.landing.href === normalized) {
      return section;
    }
    for (const page of section.pages) {
      if (page.href === normalized) {
        return section;
      }
    }
  }
  return null;
}

/**
 * The page a reader should go to next, or `null` at the end of the manual.
 *
 * Inside a section that is the entry below this one in the sidebar, starting
 * from the landing page. On a section's last page it is the landing page the
 * section names in `then` — the next section *for that reader*, which is not
 * always the one listed next.
 */
export function nextAfter(pathname: string): Entry | null {
  const section = sectionFor(pathname);
  if (section == null) {
    return null;
  }
  const normalized = normalize(pathname);
  const run = [section.landing, ...section.pages];
  const index = run.findIndex((page) => page.href === normalized);
  if (index + 1 < run.length) {
    return run[index + 1];
  }
  const then = section.then;
  if (then == null) {
    return null;
  }
  for (const candidate of sections) {
    if (candidate.landing.href === then) {
      return candidate.landing;
    }
  }
  return null;
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
