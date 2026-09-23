// @flow
//
// What a reader can do with uf, and the pages that get them there.
//
// The manual is organised by section, which answers "where is the page about
// X" — and not "I want to do Y; what do I read, in what order?". This is that
// second index: one row per goal, the pages in the order to open them, and
// the goal's status in the words the redundancy guide asks for. It renders as
// the goals page in Start and, shortened, on the home page.
//
// Every step names a page the manual lists, which
// `tests/library/docs-goals.test.js` holds — so a moved page breaks a test,
// not a reader's path. A status is a claim, so each one repeats what the pages
// it links to say about themselves and goes no further: a goal is
// `Implemented` only when every step it names describes something that works
// today; `Experimental` when a step says part of it is not established yet;
// `Planned` when the thing is not built, and then it names the issue.

export type GoalStatus = "Implemented" | "Experimental" | "Planned";

export type Step = {|
  /** What the reader does at this step: a command or a page's subject. */
  readonly label: string,
  /** A manual route, optionally with a `#fragment`. */
  readonly href: string,
|};

export type Goal = {|
  readonly title: string,
  /** One line: what the reader has at the end of the path. */
  readonly outcome: string,
  readonly status: GoalStatus,
  /** Why the status is not `Implemented`, in one line. Absent when it is. */
  readonly caveat?: string,
  readonly steps: $ReadOnlyArray<Step>,
  /** Shown in the short version on the home page. */
  readonly onHome: boolean,
|};

export const goals: $ReadOnlyArray<Goal> = [
  {
    title: "Start a new app",
    outcome: "A React application in Flow with routes, server code and a build you can deploy.",
    status: "Implemented",
    steps: [
      { label: "Install uf", href: "/guide/install" },
      { label: "uf new", href: "/guide/project" },
      { label: "Routing", href: "/guide/routing" },
      { label: "Server actions", href: "/guide/server-actions" },
      { label: "Deploy", href: "/guide/deploy" },
    ],
    onHome: true,
  },
  {
    title: "Run it every day",
    outcome: "uf dev and uf build, on a pinned runtime, with your scripts as named tasks.",
    status: "Implemented",
    steps: [
      { label: "uf dev, uf build", href: "/guide/dev" },
      { label: "Pin the runtime", href: "/guide/env" },
      { label: "uf run tasks", href: "/guide/tasks" },
    ],
    onHome: true,
  },
  {
    title: "Add tests",
    outcome: "uf test over components and server code, with requests answered by mocks, in CI.",
    status: "Implemented",
    steps: [
      { label: "uf test", href: "/guide/testing" },
      { label: "Mocks and stories", href: "/guide/mocks" },
      { label: "Run in CI", href: "/guide/ci" },
    ],
    onHome: true,
  },
  {
    title: "Type-check, format and lint",
    outcome: "One formatter, one linter and Flow's checker, the same in the editor and in CI.",
    status: "Implemented",
    steps: [
      { label: "uf fmt, uf lint", href: "/guide/format" },
      { label: "uf check", href: "/guide/check" },
      { label: "Editor (uf lsp)", href: "/guide/editors" },
      { label: "Run in CI", href: "/guide/ci" },
    ],
    onHome: true,
  },
  {
    title: "Load and change data",
    outcome: "Server Components that fetch, actions that write, and input checked where it enters.",
    status: "Implemented",
    steps: [
      { label: "Server Components", href: "/guide/server-components" },
      { label: "Server actions", href: "/guide/server-actions" },
      { label: "Validate input", href: "/guide/validation" },
      { label: "Forms", href: "/guide/form" },
      { label: "Data in the browser", href: "/guide/data" },
    ],
    onHome: true,
  },
  {
    title: "Build the interface",
    outcome:
      "Accessible components from `uf ui add`, styled with CSS or StyleX, with images and fonts handled.",
    status: "Implemented",
    steps: [
      { label: "UI components", href: "/guide/ui" },
      { label: "Component reference", href: "/reference/ui" },
      { label: "Styling", href: "/guide/styling" },
      { label: "Images and fonts", href: "/guide/assets" },
    ],
    onHome: true,
  },
  {
    title: "Serve it: a server, static files or one executable",
    outcome:
      "uf start with no bundler in the process, a static export, or a single file with the runtime inside.",
    status: "Implemented",
    steps: [
      { label: "Rendering modes", href: "/guide/rendering" },
      { label: "Choose the shape", href: "/guide/deploy#choose-the-shape" },
      { label: "Static files", href: "/guide/deploy#static-is-only-files" },
      { label: "One executable", href: "/guide/deploy#compile-one-file" },
    ],
    onHome: true,
  },
  {
    title: "Ship to Node.js, Bun, Deno, a container, the edge or serverless",
    outcome: "A directory per platform from uf build --adapter.",
    status: "Experimental",
    caveat:
      "Tests drive each adapter's output in process; no server adapter has been deployed to a real platform yet (#956).",
    steps: [
      { label: "The adapter contract", href: "/guide/deploy#the-adapter-contract" },
      { label: "uf build --adapter", href: "/reference/cli#uf-build---adapter-target" },
      { label: "What is established", href: "/guide/deploy#what-is-established" },
    ],
    onHome: true,
  },
  {
    title: "Build a phone app",
    outcome: "Native route files, Metro, navigation and tests against a native tree.",
    status: "Experimental",
    caveat: "uf cannot mount into a real native host yet; the page's status table says what works.",
    steps: [
      { label: "React Native target", href: "/guide/react-native" },
      { label: "Routing", href: "/guide/routing" },
      { label: "Testing", href: "/guide/testing" },
    ],
    onHome: false,
  },
  {
    title: "Build a terminal app",
    outcome: "React with a terminal for a host: flexbox, keyboard, focus and a cell diff.",
    status: "Implemented",
    steps: [{ label: "Terminal UI", href: "/guide/tui" }],
    onHome: false,
  },
  {
    title: "Use GraphQL and Relay",
    outcome: "Relay artifacts, typed components and request-scoped preloads.",
    status: "Experimental",
    caveat: "The RSC protocol is experimental and does not support @defer.",
    steps: [
      { label: "GraphQL and Relay", href: "/guide/graphql-relay" },
      { label: "Server Components", href: "/guide/server-components" },
    ],
    onHome: false,
  },
  {
    title: "Talk to a database with sqlc",
    outcome: "SQL you write, with Flow types generated from it.",
    status: "Planned",
    caveat: "sqlc Flow generation is tracked in #1367; today the driver is the application's own.",
    steps: [
      {
        label: "database/sql today",
        href: "/reference/std#already-in-javascript--do-not-reimplement",
      },
      { label: "Packages", href: "/reference/packages" },
    ],
    onHome: true,
  },
  {
    title: "Move an existing app",
    outcome: "A CRA, Vite or Next.js application on uf, with what differs listed up front.",
    status: "Implemented",
    steps: [
      { label: "Migrating to uf", href: "/guide/migrate" },
      { label: "What uf does not do", href: "/guide/scope" },
      { label: "uf and Next.js", href: "/guide/nextjs" },
    ],
    onHome: false,
  },
  {
    title: "Hand it to a coding agent",
    outcome: "uf mcp: the same checks, run by an agent over stdio.",
    status: "Implemented",
    steps: [
      { label: "uf mcp", href: "/guide/agents" },
      { label: "Run in CI", href: "/guide/ci" },
    ],
    onHome: false,
  },
];
