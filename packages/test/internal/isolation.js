// @flow
//
// Internal to `@uniflowed/test`: what a file is allowed to leave behind.
//
// A worker serves many files out of one process, one at a time (`../worker.js`
// says why). Everything a file registers lives in this package and is cleared
// with it; everything a file *reaches around* the package to change belongs to
// the process, outlives the file, and is handed to whichever file the schedule
// puts next in that worker.
//
// That second list is what this module is. It has been discovered three times,
// once per entry, and each time the same way — a suite that passed alone and
// failed beside another, naming the file that read the value rather than the
// file that wrote it:
//
//   * ubugeeei-prod/uf#417, `uft.stubEnv("NODE_ENV", …)` still set for the
//     next file;
//   * ubugeeei-prod/uf#581, a fake clock still installed, so the next file's
//     `setTimeout` — including the one each case is raced against — never
//     fired and the file hung with nothing on screen;
//   * ubugeeei-prod/uf#607, `document.body` still holding the markup a
//     hydration test wrote into it, so the next file's "there is one image on
//     the page" found six.
//
// Which files share a worker is decided by `.uf/test-timings.json`, so a leak
// makes the *result* of a suite depend on how the machine was loaded the last
// time it ran. That is the failure this module exists to end, and the reason
// it is a list in one place rather than three calls in `../worker.js`: the
// question "what else does a file share with the next one" now has somewhere
// to be answered, and a fourth answer is one entry rather than one more thing
// to remember.
//
// # What belongs here
//
// State that (a) the process shares, (b) a test can change from inside a file,
// and (c) nothing else puts back. Anything a file merely *reads* does not
// belong here, and neither does anything the registry already clears — a spy
// is not on this list because `reset()` is what a spy lives in.
//
// # When it runs
//
// Before the next file is imported, not after the previous one is run. A file
// that throws while loading is still a file that has run code, and it still
// hands the next one whatever that code changed; putting things back on the
// way *in* covers the load failure and the crash as well as the ordinary end.
// It also means the first file in a worker starts from the same state as the
// tenth.

import { reset } from "./registry.js";
import { resetModuleState } from "./modules.js";
import { unstubAllEnvs, unstubAllGlobals } from "./namespace.js";
// Renamed at the door, for two reasons that agree. It reads as the resets
// beside it do — `reset`, `unstubAllEnvs`, `resetModuleState` are all
// verb-first, and so is what this does to the clock. And `useRealTimers` is
// not a React hook: it is uf's own timer control, which happens to be named
// the way every runner names it, and calling it bare in a plain function is a
// `react/hooks-rules` error on the name alone. A suppression would assert
// something about this call; the name is simply accurate.
import { useRealTimers as restoreRealClock } from "./timers.js";

/**
 * One piece of process-wide state a file can change, and how to put it back.
 *
 * `what` is written for a person reading this list rather than for any code:
 * nothing branches on it, and it is here because a list of five bare function
 * references is a list nobody can check against the paragraph above it.
 */
type Shared = {|
  readonly what: string,
  readonly restore: () => void,
|};

/**
 * Everything a file shares with the file after it, in the order it goes back.
 *
 * The order matters in one place and is harmless everywhere else: the clock
 * goes back before the module stand-ins do, because a stand-in's factory runs
 * on the next import and a factory that schedules anything under a leaked fake
 * clock would schedule it into a clock nobody is going to advance.
 */
const SHARED: $ReadOnlyArray<Shared> = [
  {
    what: "the tests, hooks and spies this package registered",
    restore: reset,
  },
  {
    // `process.env` belongs to the process. `uft.stubEnv("NODE_ENV",
    // "production")` in one file is still set when the next one imports, and
    // the file that fails is the one that read it.
    what: "environment variables `uft.stubEnv` replaced",
    restore: unstubAllEnvs,
  },
  {
    // `globalThis` likewise, and worse: a stubbed `fetch` makes the next file
    // talk to a stand-in that does not know about it.
    what: "globals `uft.stubGlobal` replaced",
    restore: unstubAllGlobals,
  },
  {
    // Worse than a leaked value, and worse in a way that hides it. A leaked
    // stub makes the next file read something wrong, which arrives as an
    // assertion naming the value. A leaked clock makes the next file's
    // `setTimeout` never fire — including the one `withTimeout` races each
    // case against — so the file hangs with nothing on screen until `uf`'s own
    // deadline kills the worker, and the report names the file that waited
    // rather than the file that stopped time.
    what: "the clock, whatever `uft.useFakeTimers` did to it",
    restore: restoreRealClock,
  },
  {
    // A worker serves many files out of one module registry, so this is the
    // difference between "one file at a time" and "one file's mocks at a
    // time".
    what: "modules `uft.mock` stood in for",
    restore: resetModuleState,
  },
  {
    what: "the document, if this process has one",
    restore: restoreDocument,
  },
];

/**
 * Hand the next file a document nobody else has written to.
 *
 * The document is process-wide in a way that is easy to miss, because nothing
 * in this package installs it: `@uniflowed/react-testing` puts one on the
 * global object the first time a test renders and deliberately keeps it for
 * the life of the process — replacing it would strand every React root already
 * mounted in the old one. So one document serves every file a worker runs, and
 * what a file leaves in it is what the next file queries.
 *
 * Its *contents* are put back rather than the document itself, and "back"
 * means empty: a file is handed the body it would have had if it had installed
 * the document itself. `cleanup()` already unmounts what `render` mounted, and
 * that is not the leak — the leak is markup a test wrote into the body by
 * hand, which a hydration test must do because hydration is React attaching to
 * markup that is already there. `rsc-split.test.js` and `streaming.test.js`
 * both `replaceChildren` into the body, and the file after them in that worker
 * started with somebody else's page.
 *
 * Read through `globalThis` and guarded by `typeof`, because a worker running
 * a suite that never renders has no `document` at all and must not pay for
 * one — and because on a host that *is* a browser this is the page, whose body
 * a run of `uf test` has no business emptying. It only ever clears a body that
 * a test process is using as scratch space, which is every case this can
 * reach: the shim's document, or a page the project chose to run its own tests
 * in.
 */
function restoreDocument(): void {
  if (typeof globalThis.document === "undefined") {
    return;
  }
  const body = globalThis.document.body;
  if (body == null) {
    // A document a parser produced need not have one, and a document with no
    // body is a document with nothing to put back.
    return;
  }
  body.replaceChildren();
  // The attributes too, and not for tidiness: `<body class="dark">` is how a
  // theme test says what it is testing, and a file that leaves one behind
  // makes the next file's "the page is in light mode" false for a reason it
  // cannot see. Read into an array first — removing an attribute while
  // iterating a live list is the loop that skips every other entry.
  for (const name of [...body.getAttributeNames()]) {
    body.removeAttribute(name);
  }
}

/**
 * Put back everything the file that just ran may have changed.
 *
 * Called by `../worker.js` before it imports the next file. Nothing here
 * reports a file for having changed any of this: a file is *allowed* to — that
 * is what the `uft` namespace is for — and the contract is that the change
 * does not outlive the file, not that it never happened.
 *
 * Every entry is attempted even after one has thrown, and the first failure is
 * raised afterwards under the name of what it could not put back. Stopping at
 * the first would leave the four entries behind it un-restored, which is the
 * defect this module exists to prevent arriving by a new route — and a bare
 * throw from one of these used to say only that something in the runner failed
 * between two files.
 */
export function restoreSharedState(): void {
  let failure: { readonly what: string, readonly thrown: mixed } | null = null;
  for (const shared of SHARED) {
    try {
      shared.restore();
    } catch (thrown) {
      failure ??= { what: shared.what, thrown };
    }
  }
  if (failure != null) {
    const cause = failure.thrown;
    const message = cause instanceof Error ? cause.message : String(cause);
    throw new Error(`@uniflowed/test could not put back ${failure.what}: ${message}`);
  }
}
