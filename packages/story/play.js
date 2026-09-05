// @flow
//
// `@uniflowed/story/play`: the half of a story that makes it a test.
//
// A story with props is a picture: it proves the component renders, and
// nothing about what happens when somebody uses it. A play function is what
// closes that gap — it is handed the story's own mounted markup and drives it
// the way a person would, then asserts.
//
//   Submitted: {
//     play: async ({ canvas, user, step }) => {
//       await step("fills the form", async () => {
//         await user.type(canvas.getByLabelText("Email"), "a@b.test");
//       });
//       await user.click(canvas.getByRole("button", { name: "Save" }));
//       expect(await canvas.findByRole("status")).toBeTruthy();
//     },
//   }
//
// This module owns three things: what that function is handed, what a step
// means, and what a failure inside one says.
//
// # The canvas is the story, not the document
//
// `canvas` queries the container this story was mounted into, not the whole
// page. `screen` is still there for a portal — a dialog renders outside its
// parent by design — but the default is scoped, because a story that finds
// the *previous* story's button and passes is worse than one that fails.
//
// # Steps exist for the failure message
//
// A play function that does five things and fails on the fourth reports one
// assertion error and no account of how it got there. `step` names a stretch
// of it, so a failure reads `button--submitted > fills the form: unable to
// find a label "Email"`. Nested steps join with the same separator.
//
// Nothing else about a step is special: it does not retry, it does not time
// out on its own — the test runner already owns the timeout — and it does not
// group output. It is a label, and a label is what was missing.
//
// # No assertion library here
//
// A play function asserts with whatever the file it lives in imports:
// `expect` from `@uniflowed/test` in a test, or a bare `throw` in a story
// file that must not depend on the runner. This module only reports what came
// out, so nothing here decides how a project writes an assertion — and a
// story file stays importable by a tool that has no test runner in it.

import type { Queries } from "@uniflowed/react-testing";
import { userEvent, within } from "@uniflowed/react-testing";

/** How steps are joined into the label a failure carries. */
const STEP_SEPARATOR = " > ";

/** A named stretch of a play function. */
export type Step = (name: string, body: () => mixed) => Promise<void>;

/**
 * What every play function is handed, minus the props.
 *
 * Split out from [`PlayContext`] because it is the part that does not depend
 * on the story's type parameter — `render.js` builds one of these knowing
 * only a DOM node, and `story.js` closes over the typed props to complete it.
 */
export type PlayStage = {|
  /** The element this story was mounted into. */
  readonly container: Element,
  /** Queries scoped to `container`. */
  readonly canvas: Queries,
  /** What a person did: click, type, tab, hover. */
  readonly user: typeof userEvent,
  /** Name a stretch of the play, so a failure says where it was. */
  readonly step: Step,
|};

/** What a play function declared on a typed story set is handed. */
export type PlayContext<Props> = {|
  ...PlayStage,
  /** The props this story was rendered with. */
  readonly props: Props,
|};

/**
 * A play function with its story's props already inside it.
 *
 * This is the shape a resolved [`Story`](./story.js) holds: `story.js` wraps
 * the caller's typed function so that everything downstream can call it with
 * a stage and nothing else.
 */
export type PlayFunction = (stage: PlayStage) => mixed;

/**
 * A play function's failure, told where it happened.
 *
 * The original message is kept in full and the original error in `cause`,
 * because the useful half of an assertion failure is the assertion's own
 * account of what it expected. What this adds is the story id and the step
 * path, which the assertion cannot know.
 */
export class StoryPlayError extends Error {
  /** The story that was playing. */
  readonly story: string;
  /** The step path, or `null` when the failure was outside every step. */
  readonly step: string | null;

  constructor(story: string, step: string | null, cause: mixed) {
    const where = step == null ? story : `${story}${STEP_SEPARATOR}${step}`;
    super(`${where}: ${messageOf(cause)}`);
    this.name = "StoryPlayError";
    this.story = story;
    this.step = step;
    this.cause = cause;
  }
}

/**
 * Build the stage for a story mounted into `container`.
 *
 * The `step` this returns writes into `path`, which [`runPlay`] reads when
 * something throws. A step that fails leaves its name in place on purpose:
 * the failure is reported from the innermost step that was running, not from
 * wherever the stack happened to unwind to.
 */
export function createStage(container: Element): {|
  readonly stage: PlayStage,
  readonly stepPath: () => string | null,
|} {
  const path: Array<string> = [];

  const step: Step = async (name, body) => {
    path.push(name);
    await Promise.resolve(body());
    path.pop();
  };

  return {
    stage: {
      container,
      canvas: within(container),
      user: userEvent,
      step,
    },
    stepPath: () => (path.length === 0 ? null : path.join(STEP_SEPARATOR)),
  };
}

/**
 * Run `play` for the story called `story`, on `stage`.
 *
 * Always awaits, even for a synchronous play function, so that a play that
 * grows an `await` later does not change when its failure surfaces —
 * a synchronous throw and a rejected promise both arrive here.
 */
export async function runPlay(
  play: PlayFunction,
  story: string,
  stage: PlayStage,
  stepPath: () => string | null,
): Promise<void> {
  try {
    await Promise.resolve(play(stage));
  } catch (error) {
    throw new StoryPlayError(story, stepPath(), error);
  }
}

/** What `error` says, whatever it is. */
function messageOf(error: mixed): string {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
