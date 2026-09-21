# Commonplace · GraphQL and Relay

A separate version of [Commonplace](../simple-sns), keeping its visual design and local media. The frontend uses Flow, React, StyleX, Relay 21.0.1, and streamed React Server Components. An independent Go service owns the GraphQL schema, authentication, posts, reactions, and conversations.

## Run from this checkout

Use Node 24 or newer, Go 1.25 or newer, and a current workspace `uf` binary. The `file:` dependencies intentionally use the packages in this repository.

```sh
npm ci
npm ci --prefix examples/simple-sns-graphql
uf --cwd examples/simple-sns-graphql run relay
```

Start the backend in one terminal, then the frontend in another:

```sh
uf --cwd examples/simple-sns-graphql run backend
uf --cwd examples/simple-sns-graphql dev --host 127.0.0.1 --port 4182
```

Open `http://127.0.0.1:4182` and create an account. Seed members have no passwords. The backend listens at `http://127.0.0.1:4183`; `SNS_GRAPHQL_LISTEN` changes that address and `SNS_GRAPHQL_ENDPOINT` points the frontend at the upstream service. Set the endpoint for both build and start when using another service.

```sh
uf --cwd examples/simple-sns-graphql build
uf --cwd examples/simple-sns-graphql start --host 127.0.0.1 --port 4182
```

This is a demonstration service with in-memory data. Restarting it clears accounts, sessions, posts, and messages. A deployed application supplies durable storage, migrations, rate limiting, recovery and verification flows, and its own backend operations. Flow does not provide an ORM or a migration runner here.

## Boundaries

- `backend/` is its own Go module. `graph-gophers/graphql-go` executes the schema; the application owns authorization and data. No database abstraction is implemented in Flow.
- `app/graphql/$route.js` is a bounded BFF: same-origin JSON POST, a 64 KiB request limit, an upstream timeout, and forwarding only the application's HttpOnly session cookie. Its upstream URL is server configuration.
- `app/_server/relay.server.js` uses upstream `createServerEnvironment`. React's request cache creates a separate Relay environment for each RSC request. No visitor shares a module-level store.
- Routes and the shared frame are server components. `$layout.js` mounts the browser's Relay provider once; headings, navigation, channels, URL state and guest prompts never enter a client bundle. A `"use client"` file exists only where the browser holds state: a mutation, a draft, a video, or records that mutations rewrite.
- A server component calls `relay.serverPreloadQuery` beside the island that needs the data, and that island reads its reference with `useQueryFromServer`, without a duplicate initial fetch. Every preload of a route starts in the same render pass. The feed reads identity, the composer's viewer and the timeline; the inbox streams its thread list and the selected conversation into separate Suspense boundaries; login, signup and clips read nothing but the frame's identity.
- The frame's `preloadSession()` reference feeds both of its identity islands. `SignedIn` reads the same reference to choose between two server-rendered subtrees, so the inbox and settings gate on identity without moving their layout into the browser.
- Each island's query selects its own fields and spreads its children's fragments: timeline to post to avatar, threads to thread, conversation to message. Generated types live in the nearest `__generated__/` directory. Mutations live beside the component that commits them.
- Relay owns optimistic reaction rollback and normalized records. Successful publishing refreshes the selected feed; private messages use a store updater. Request IDs make write retries idempotent in the backend.
- Signing in or out starts a fresh document and Relay environment. Settings and conversations stay scoped to the authenticated backend session.

## Relay compiler and RSC

Run `uf run relay` after changing a tag or the schema. `uf run relay -- --validate` detects stale artifacts. `relay.config.json` pins modern Flow output with `featureFlags.flow_modern_syntax`; generated SignedSource files belong to Relay and must not be hand edited. `uf fmt` preserves them, while `uf check` checks their imported types.

The Vite integration invokes the official Relay Babel plugin after Flow lowering. It reads the application's single-project Relay config and binds `graphql` tags to artifacts. Relay 21 uses separate variables and response generic parameters for mutation hooks.

RSC entry points retain upstream's `EXPERIMENTAL` suffix. This example pins 21.0.1 and uses ordinary paginated query fields, fragments and mutations. It does not use `@defer`, which upstream's RSC preload protocol does not support, or promise coverage of every experimental connection/incremental-delivery combination. See the [Relay 21 release](https://relay.dev/blog/2026/05/18/relay-21/).

## Styles and verification

Shared component styles live in `app/_shared/commonplace.stylex.js`. Global resets, descendant selectors, responsive rules and video state remain in `base.css`. The existing SNS's desktop and mobile appearance was preserved during the shared StyleX migration.

The CI task runs compiler validation, Flow checking, lint, a production build, Go tests with the race detector, and Chromium against both `uf dev` and `uf start`:

```sh
uf run example:sns:graphql
```

It verifies RSC preload without a duplicate fetch, signup, publishing, optimistic reactions, Flight navigation, private DM/settings, logout, mobile layout and BFF request rejection. `UF_BROWSER` selects a Chromium executable. Failure logs and screenshots are written to `output/relay-sns`.

All portraits, videos and posters retain the [original media credits](public/media/CREDITS.md).
