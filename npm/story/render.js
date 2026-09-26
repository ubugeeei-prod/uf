// @flow
//
// `@uniflowed/story/render`: putting one story on screen, and taking it down.
//
// Everything about a story that involves *time* is here: when its handlers
// start intercepting, when React mounts it, when its play function runs, and
// what is put back afterwards. `story.js` is data and this is the lifetime.
//
//   const mounted = mountStory(findStory(buttonStories, "Pending"));
//   try {
//     expect(mounted.canvas.getByRole("button")).toBeDisabled();
//     await mounted.play();
//   } finally {
//     mounted.unmount();
//   }
//
// # One renderer, for the test and for the person
//
// A story that only a bespoke UI can render is a story nobody runs in CI, so
// there is exactly one path here and both callers take it. `@uniflowed/test`
// reaches it through [`mountStory`] and asserts on `canvas`;
// [`renderStoryToHtml`] is the same mount, serialised, which is what a static
// story page or a review artefact needs. Neither is a second implementation
// of the first, so neither can drift from it.
//
// The DOM is `@uniflowed/react-testing`'s — a real document, installed on
// first render, with React told it is under test so an unwrapped update is
// still reported. A story therefore mounts the same way a component test
// does, on Node.js, Bun or Deno, with nothing configured.
//
// # Mocks start before the mount, and stop after the unmount
//
// A component that fetches does it in an effect, and `render` flushes effects
// before it returns — so a registry installed after the mount would miss the
// first request every time. It is installed first, and closed by `unmount`,
// which is also what puts the platform's `fetch` back.
//
// A story that declares no handlers installs **no** interception at all. It
// is not given an empty registry that rejects everything: a story with no
// mocks reaches the network exactly as the application would, which is the
// honest default and the only one that leaves `passthrough` meaning
// something. A story that *does* declare handlers gets
// `@uniflowed/mock`'s own default, `onUnhandledRequest: "error"` — having
// said what this story talks to, a request to anything else is a finding.
//
// # Decorators wrap outside-in
//
// The set's decorators are applied around the story's, and within each list
// the first written is the outermost. `[withTheme, withRouter]` reads as
// theme outside router, and that is what it does.

import { mock } from "@uniflowed/mock";
import type { MockRegistry, RecordedRequest } from "@uniflowed/mock";
import type * as React from "@uniflowed/react";
import type { Queries } from "@uniflowed/react-testing";
import { render } from "@uniflowed/react-testing";

import { createStage, runPlay } from "./play.js";
import type { Decorator, Story } from "./story.js";

/** A story on screen, and everything a caller can do with it. */
export type MountedStory = {|
  /** The story that was mounted. */
  readonly story: Story,
  /** The element it was mounted into. */
  readonly container: Element,
  /** Queries scoped to `container`. */
  readonly canvas: Queries,
  /**
   * Requests this story made, in request order.
   *
   * The registry's live log, not a copy, so a caller that holds on to it
   * across an interaction sees what the interaction asked for. Empty and
   * permanently so when the story declares no handlers — nothing is watching.
   */
  readonly requests: $ReadOnlyArray<RecordedRequest>,
  /**
   * Run the story's play function, if it has one.
   *
   * Resolves immediately when it has none, so a caller never has to ask.
   * Throws a [`StoryPlayError`](./play.js) naming the story and the step.
   */
  readonly play: () => Promise<void>,
  /** The story's markup, as it stands. */
  readonly html: () => string,
  /** Take it down and stop intercepting. Safe to call twice. */
  readonly unmount: () => void,
|};

/**
 * Mount `story` and hand back what it produced.
 *
 * Synchronous, because mounting is: `render` wraps the work in React's `act`,
 * which runs the render, the effects and the microtasks React queued before
 * it returns. What a story's effects then *await* — a request, a timer — is
 * not finished, which is what `findBy…` and [`MountedStory.play`] are for.
 *
 * Mounting a second story takes the first down: `@uniflowed/react-testing`
 * cleans up before it renders, and a document holding two stories makes
 * "there is one Save button" false for reasons that have nothing to do with
 * the story being read.
 */
/**
 * The registry the last mounted story installed, if it had one.
 *
 * `installFetch` refuses to nest, so a second story with mocks cannot listen
 * while the first is still listening — and a test that mounts and asserts
 * without unmounting is the ordinary shape, so "the caller will unmount" is
 * not something this can rely on. Mounting closes whatever is still
 * installed, exactly as `@uniflowed/react-testing` takes down the tree the
 * story before it left.
 */
let active: MockRegistry | null = null;

export function mountStory(story: Story): MountedStory {
  active?.close();
  active = null;

  const registry = story.mocks.length > 0 ? mock(...story.mocks) : null;
  if (registry != null) {
    registry.listen();
    active = registry;
  }

  let result;
  try {
    result = render(decorate(story.element(), story.decorators));
  } catch (error) {
    // The registry is listening and the story never mounted, so nothing will
    // ever call `unmount`. Leaving it installed would hand the *next* story a
    // `globalThis.fetch` belonging to a story that failed to render, and
    // `listen()` refuses to nest — so the next mount would fail with an
    // unrelated message.
    registry?.close();
    if (active === registry) {
      active = null;
    }
    throw error;
  }

  const { stage, stepPath } = createStage(result.container);
  let live = true;

  return {
    story,
    container: result.container,
    canvas: stage.canvas,
    requests: registry?.requests ?? [],
    play: async () => {
      if (story.play != null) {
        await runPlay(story.play, story.id, stage, stepPath);
      }
    },
    html: () => result.asFragment(),
    unmount: () => {
      if (!live) {
        return;
      }
      live = false;
      result.unmount();
      registry?.close();
      // Only when it is still this story's: a late unmount must not take away
      // the registry a story mounted afterwards is using.
      if (active === registry) {
        active = null;
      }
    },
  };
}

/**
 * The story's markup, for something that is not a test.
 *
 * A static story page, a review artefact, a diff in a pull request: all of
 * them need the same string, and none of them should mount React themselves.
 *
 * `play` is off by default. The markup of a story *as declared* is what a
 * catalogue shows; the markup after it has been driven is a different and
 * equally useful picture, and the caller is the one who knows which they
 * meant.
 */
export async function renderStoryToHtml(
  story: Story,
  options?: {| readonly play?: boolean |},
): Promise<string> {
  const mounted = mountStory(story);
  try {
    if (options?.play === true) {
      await mounted.play();
    }
    return mounted.html();
  } finally {
    mounted.unmount();
  }
}

/**
 * Wrap `node` in `decorators`, first outermost.
 *
 * Applied back to front so that `decorators[0]` ends up furthest from the
 * component. A `reduceRight` would say the same thing in one line and
 * allocate an intermediate for each step; a story is rendered per assertion
 * in a watch loop, so the loop stays.
 */
function decorate(node: React.Node, decorators: $ReadOnlyArray<Decorator>): React.Node {
  let wrapped = node;
  for (let index = decorators.length - 1; index >= 0; index -= 1) {
    wrapped = decorators[index](wrapped);
  }
  return wrapped;
}
