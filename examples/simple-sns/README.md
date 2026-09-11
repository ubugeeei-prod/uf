# Commonplace

A community application built with uf, Flow, React, and `@uniflowed/ui`. Public notes and short videos sit alongside account profiles and private conversations. The responsive interface uses neutral surfaces, real photography, translucent navigation, and fine separators, with no decorative shadows or gradients.

## Run

From the repository root, using the workspace uf binary:

```sh
./target/debug/uf dev --cwd examples/simple-sns
./target/debug/uf build --cwd examples/simple-sns
UF_SIMPLE_SNS_DB=/tmp/commonplace.sqlite ./target/debug/uf start --cwd examples/simple-sns --host 127.0.0.1 --port 4181
```

Use the same database path between restarts. Without the environment variable, storage is `.uf/commonplace.sqlite` relative to the application working directory. The four seed members have no passwords; create your own account at `/signup`.

| Route | Behavior |
| --- | --- |
| `/` | Searchable, paginated channel feed; optimistic publishing and appreciation |
| `/clips` | Vertical scroll-snap video feed, keyboard navigation, play/pause and mute controls |
| `/messages` | Independently loaded inbox and authorized private conversation |
| `/settings` | Persisted profile and account fields |
| `/login`, `/signup` | Account creation and cookie-based sessions |
| `/auth/session` | Bounded, same-origin HTTP authentication endpoint |

## Client and server boundary

Route loaders start independent reads before awaiting the session needed by the shell. `social-queries.js` and `social-actions.js` are `"use server"` modules: the browser receives server references, never the SQLite repository, password hashing, or session implementation. Every private read and every mutation resolves identity from the current request; neither a member ID supplied by a client nor an action's previous state is trusted.

uf currently splits route loaders and server references and streams deferred loader values. It does **not** transport an arbitrary React Flight component tree. The route's renderable module and its interactive dependencies are hydrated on the client. HTTP `$route` handlers are excluded from the client route table, with a Vite regression guarding that boundary.

The server layers are deliberately small:

- `server/database.server.js` owns SQLite, schema, seeding, and transactions.
- `server/repository.server.js` uses parameterized queries and returns public DTOs. Conversation membership scopes every read and write.
- `server/session.server.js` owns asynchronous scrypt password verification, hashed expiring session tokens, rotation, and persistent login throttling.
- `server/programs.server.js` expresses mutations with `@uniflowed/effect`, typed expected failures, and injected identity/store services. Validation runs before store access. Unexpected defects remain defects.
- `server/input-effect.server.js` converts expected input exceptions from `trySync`/`tryPromise` into typed failures without treating database faults as validation.
- `server/run-mutation.server.js` provides request-scoped dependencies and translates the exit into a safe public action result.

Synchronous transactions use `trySync` and `ensuring` to retain the original error and roll back partial writes. The authentication stream also releases its reader through `ensuring`. Client transport, navigation, and media failures are evaluated through Effect exits; the example has no `try/catch` blocks.

Publication and message request IDs make retries idempotent. Reusing an ID for different content fails. Appreciation sets an intended state per member. Database transactions keep conflicting profile updates and partial writes atomic. Authentication issues only an HttpOnly, SameSite cookie; tokens never enter React state or browser storage. HTTPS requests receive Secure cookies.

## React and Flow design

`Session`, `ActionResult`, `ConversationData`, and `Playback` are discriminated unions. Exhaustive Flow `match` expressions and statements handle the variants directly, including in JSX. Useful `renders` slots constrain post lists, message logs, form controls, and empty-state actions. Negative type fixtures prove these constraints reject invalid composition.

The shell, search, channel controls, and composer remain outside the feed's Suspense boundary. The inbox list and selected conversation each own a boundary and retry. A loader creates each initial promise; render reads it with `use` and never starts a request. Explicit retries replace only that region's resource. A loader refresh adopts its new promise. Each request settles into a ready/failed ADT: functional components render recovery with `match`, while Suspense handles pending work. Unexpected render defects use the router’s `$error.js`. There are no class components in the example. Skeletons reuse the loaded content's row and field geometry.

`useActionState` owns submission status. Optimistic updates roll back on failure while controlled drafts remain available; successful responses become the committed state once. Transport errors pass through one small adapter. `Activity` preserves the collapsed composer and its draft. `ViewTransition` scopes note and composer changes. Reduced-motion preferences are respected. There is no manual `memo`, `useMemo`, or `useCallback` in this example; the compiler handles optimization where supported.

`@uniflowed/ui` supplies avatar, field, alert, skeleton, and collapsible primitives. Effects synchronize external browser media, visibility, observers, and conversation scroll position; server data does not get copied through effects.

## Source conventions

Keep blank lines between declarations and logical steps, and between CSS rules. Public functions, components, and model types have documentation comments describing their contracts; authorization, retry ownership, and cleanup behavior belong beside the implementation. Run `uf fmt` to preserve the repository style. Comments should explain constraints and behavior instead of repeating the signature.

## Clips and media

Only the visible clip has a video `src`. Switching clips pauses and releases the previous video; leaving the route cleans up playback and the observer. Background tabs pause playback. Reduced-motion users start with a poster and an explicit play control. Autoplay rejection and media failure have actionable states. These are three curated stock clips, not an upload, recommendation, or transcoding service.

All portraits, videos, and posters are local assets. Attribution and the original media licenses are in [public/media/CREDITS.md](public/media/CREDITS.md).

## Verification

```sh
./target/debug/uf check examples/simple-sns
./target/debug/uf test tests/library/examples.test.js tests/library/simple-sns-components.test.js tests/library/simple-sns-types.test.js
./target/debug/uf test packages/react-testing/react-testing.test.js packages/vite/rsc-split.test.js
```

The tests exercise authorization, session isolation and expiry, persistence after reopening the database, conflicting writes, idempotent retries, Effect dependency injection, optimistic rollback, preserved drafts, independent Suspense reveal/retry, and invalid Flow composition. They use isolated temporary databases.

This is a single-instance SQLite application example. Its account model does not include email verification, password recovery, or MFA. Deployment infrastructure, distributed throttling, backups, and user-media ingestion remain application-specific concerns.
