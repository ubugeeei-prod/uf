// @flow
//
// `@uniflowed/story/runner`: stories as tests `uf test` runs.
//
// A story is already a rendered state with its setup declared beside it. This
// module is the twenty lines that turn that into test results, so a project
// does not write the same mount-play-unmount block once per story.
//
//   import { describe, it } from "@uniflowed/test";
//   import { describeStories, storyTest } from "@uniflowed/story/runner";
//   import { stories } from "./_uf.story.js";
//
//   it("renders every Button story", storyTest(findStory(stories, "Primary")));
//   describe("Button", () => {
//     describeStories(stories);
//   });
//
// # Why this is a separate entry point
//
// It is the only module in the package that imports `@uniflowed/test`.
// Everything else — declaring, collecting, rendering — works with no test
// runner in the process, which is what lets a story file be imported by a
// documentation build or a story page. Putting the bridge behind
// `@uniflowed/story/runner` means a consumer that does not want the runner
// never resolves it. `@uniflowed/form/validator` is the same arrangement for
// the same reason.
//
// # A mount is already an assertion
//
// A story with no play function still fails when the component throws, when a
// required prop is missing at runtime, when an effect rejects, or when it
// requests something its handlers do not cover — `mountStory` installs the
// story's mocks with `onUnhandledRequest: "error"`. That is a real test, and
// it is the one Storybook calls a smoke test. A play function is what turns
// it from "it rendered" into "it works".
//
// # What `uf test` can and cannot see here
//
// `uf test` discovers test declarations by scanning source text for `it(` and
// `describe(` with a **string literal** first argument, and a file with no
// such declaration is not run at all. [`describeStories`] registers its cases
// from the set, so their names are not literals — which means a file whose
// only content is a `describeStories` call is skipped silently, reported as
// zero files and zero tests, and exits 0.
//
// So a story test file must contain at least one literal declaration, and the
// shape above is the recommended one: a literal `describe` is not enough,
// because discovery counts only `it` and `test`. Once the file is picked up,
// every case [`describeStories`] registered runs and is reported normally —
// the worker runs what the file registered, not what discovery predicted.
//
// [`storyTest`] exists for the other half of that: it returns the body, so a
// project that wants one literally named test per story can write one and
// keep `uf test -t` able to select it.

import { describe, it } from "@uniflowed/test";

import { mountStory } from "./render.js";
import type { Story, StorySet } from "./story.js";

/**
 * The body of one story's test: mount it, play it, take it down.
 *
 * The unmount is in a `finally` because a failed assertion inside a play
 * function would otherwise leave the story mounted and its handlers
 * installed — and `@uniflowed/mock` refuses to nest, so the *next* story
 * would fail with a message about interception rather than about itself.
 */
export function storyTest(story: Story): () => Promise<void> {
  return async () => {
    const mounted = mountStory(story);
    try {
      await mounted.play();
    } finally {
      mounted.unmount();
    }
  };
}

/**
 * Register one test per story in `set`, under a suite named after it.
 *
 * Read the module docs before relying on this as a file's only content: the
 * names come from the set rather than from source text, so `uf test` will not
 * discover the file on its own.
 */
export function describeStories(set: StorySet): void {
  describe(set.title, () => {
    for (const story of set.stories) {
      it(story.name, storyTest(story));
    }
  });
}
