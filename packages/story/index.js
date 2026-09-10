// @flow
//
// `@uniflowed/story`: a named state of a component that a person and a test
// can both reach.
//
// uf already has a test runner, a real DOM, queries a person would recognise,
// and request mocking. So the question this package has to answer is what a
// story is still worth once all of that exists — and the answer is not "a
// nicer way to render a component in a test", because `render(<Button />)`
// was already that.
//
// What a story adds is that the state has a **name outside the file that
// produced it**. `button--pending` is a thing a CI job can print, a visual
// baseline can be filed under, a URL can carry and a reviewer can ask for.
// The props and the setup that state needs are declared once, beside the
// name, instead of being rebuilt in every test that wants it — and because
// they are declared as data, the same declaration serves the assertion and
// the picture.
//
//   // src/components/$story.js
//   export const stories = defineStories({
//     title: "Button",
//     component: Button,
//     props: { label: "Save", pending: false },
//     stories: {
//       Primary: {},
//       Pending: { props: { pending: true } },
//       Saved: {
//         mocks: [http.post("/save", () => HttpResponse.json({ ok: true }))],
//         play: async ({ canvas, user }) => {
//           await user.click(canvas.getByRole("button", { name: "Save" }));
//           if ((await canvas.findByRole("status")).textContent !== "Saved") {
//             throw new Error("the button did not report success");
//           }
//         },
//       },
//     },
//   });
//
// That file is three things at once: the catalogue entry a person browses,
// the fixture a test mounts, and — for `Saved` — a test in its own right.
//
// # Why this is Flow now
//
// It used to be a declaration whose every function returned
// `nativeRuntimeRequired(…)` behind an opaque `NativeHandle`, which is to say
// it was a contract with nothing behind it. Nothing here needs to be native
// and nothing here would be faster if it were: declaring a story builds a
// closure, rendering one is React's work in a DOM, and playing one is the
// same event dispatch `@uniflowed/react-testing` already does. The one part
// of a story system that *is* a hot path — walking a repository to find every
// story file — is discussed under `collect.js`, which says plainly that the
// walk belongs in Rust when there is a `uf story` command to own it.
//
// So the handle is gone rather than preserved. A story is a value: it can be
// built, passed, filtered and rendered by ordinary code, which is what makes
// the same declaration reachable from a test, from a page and from a script.
//
// # How the package is laid out
//
// Five modules beside this one, split by what each decides:
//
// - `story.js` — **declaring**: `defineStories`, what a story inherits from
//   its set, and where the `Props` type parameter is checked and why it is
//   then erased. Pure data; declaring a story runs nothing.
// - `collect.js` — **finding**: `$story.js`, the repository's own reserved
//   name grammar, the walk, and what makes a story file valid.
// - `render.js` — **rendering one**: the mock lifetime, the decorators, the
//   mount, and the markup. Everything here is about *time*.
// - `play.js` — **driving one**: what a play function is handed, what a step
//   means, and what a failure inside one says.
// - `runner.js` — **`@uniflowed/test`**: one test per story. A separate entry
//   point, so a consumer that does not want the test runner never resolves
//   it. Not re-exported below, for the same reason.
//
// There is no `internal/`. Every module here is a reasonable thing to import
// on purpose: a documentation build wants `collect.js` and `render.js` and
// has no use for the runner, and a test wants the runner and never walks a
// directory.
//
// # Readiness
//
// **Implemented and tested.** Declaring a component's stories with complete
// props on the set and a partial override per story, with per-story and
// per-set decorators, mocks and play functions; names defaulting to the
// declaration key; stable `title--name` ids. Collecting them: the
// `$story.js` reserved name with the router's variant vocabulary, a
// bounded walk that skips `node_modules` and symlinks, loading every story
// set a file exports, and rejecting two stories that share an id. Rendering
// one into `@uniflowed/react-testing`'s DOM, with the story's handlers
// listening before the mount and `globalThis.fetch` put back on unmount, and
// with the recorded requests exposed. Play functions with a scoped `canvas`,
// `userEvent`, named steps, and a failure that carries the story id, the step
// path and the original error. `renderStoryToHtml` for something that is not
// a test. One `it` per story through `@uniflowed/story/runner`.
//
// **Experimental.** The `$story.js` name itself. It follows uf's reserved
// grammar — `$<role>[.<variant>].js` — but `story` is not yet one of the
// roles `crates/uf_router/src/reserved.rs` defines, and that file is the
// grammar's single source of truth for `uf create`, the router and the
// linter. Until a `story` role is added there, **`uf lint` reports
// `router/reserved-files` on every `$story.js`**: the name is right and
// the linter has not been told. The alternative was to invent a second
// convention (`*.stories.js`) that no uf tool knows about, which is worse.
//
// `describeStories` is experimental for a different reason, and it is a
// property of `uf test` rather than of this package: discovery scans source
// text for `it(` with a string-literal name, so a file whose only content is
// a `describeStories` call is skipped — silently, reporting zero files and
// exiting 0. `runner.js` documents the shape that works today and
// `storyTest` is the escape hatch.
//
// **Not implemented.** There is no `uf story` command, no development server,
// no browser canvas and no static story site: this package produces the index
// and the markup those would need, and nothing renders them for a person yet
// beyond a string. `withBrowser` is deliberately gone rather than carried
// over — `@uniflowed/browser` is still a declaration whose every function
// throws, so a story that claimed to drive a real browser would be claiming a
// capability that does not exist. For the same reason nothing here talks to
// `@uniflowed/vrt`; `storyId` is exported so that a baseline can be filed
// under the same name when it does.
//
// Also absent, and each for a reason rather than by oversight: no Storybook
// CSF compatibility, no `argTypes`, controls or knobs (a control panel needs
// the canvas that does not exist), no MDX or autodocs, no addon protocol, no
// global decorators or a project-level `preview.js` (a story's setup is
// declared in the story's own file, where it can be read), no composition of
// remote catalogues, and no story-level snapshot testing —
// `@uniflowed/test`'s snapshots work on the string `renderStoryToHtml`
// returns. Only the default variant of the reserved name is rendered:
// `$story.native.js` is recognised and skipped, because a React Native
// renderer does not exist here either. Nothing renders a story through RSC or
// server rendering; a story mounts on the client, which is what
// `@uniflowed/react-testing` provides.

export type {
  Decorator,
  Story,
  StoryDeclaration,
  StoryProps,
  StorySet,
  StorySetConfig,
} from "./story.js";
export type { FindOptions, StoryIndex, StoryVariant } from "./collect.js";
export type { PlayContext, PlayFunction, PlayStage, Step } from "./play.js";
export type { MountedStory } from "./render.js";

export { STORY_SET, defineStories, findStory, isStorySet, storyId } from "./story.js";
export {
  STORY_FILE,
  STORY_ROLE,
  classifyStoryFile,
  collectStories,
  findStoryFiles,
  indexStories,
  isStoryEntry,
  loadStoryFile,
} from "./collect.js";
export { StoryPlayError } from "./play.js";
export { mountStory, renderStoryToHtml } from "./render.js";
