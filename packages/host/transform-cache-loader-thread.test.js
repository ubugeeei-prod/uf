// @flow
//
// The transform cache, through the loader thread `register()` starts: what an
// older Node gets, and what `@uniflowed/vite`'s driver registers. Every Node
// this suite runs on has `registerHooks`, so the in-thread file alone would
// leave this loader unexercised — which is how a loader stops working without
// anybody noticing.
//
// The bodies are `../../tests/library/transform-cache.js`, shared with
// `./transform-cache.test.js`; the names are here because `uf test` reads this file's own
// source to decide whether it holds tests at all.

import { describe, it } from "@uniflowed/test";

import {
  LOADER_THREAD,
  resolvesTheProjectsCompilerRuntime,
  servesASecondRunFromDiskRatherThanCompilingAgain,
  compilesAgainWhenUfItselfWasRebuilt,
  stillHasABuildsEntriesWhenThatBuildComesBack,
  doesNotServeWhatItCachedWhenItCannotTellWhichUfCompiledIt,
  doesNotServeABuildsEntriesToTheBuildThatReplacedItMidrun,
  doesNotServeWhatItCachedThroughABinaryThatCanNoLongerRun,
  identifiesTheBinaryAHostStartedByHandFindsOnPath,
  failsTheImportOfAModuleTheCompilerRefusedWithWhatTheCompilerSaid,
  leavesACommonjsModuleThatSomethingRequiresToNode,
} from "../../tests/library/transform-cache.js";

describe("the transform cache, through the loader thread", () => {
  it("uses an installed compiler runtime before the standalone test fallback", () => {
    resolvesTheProjectsCompilerRuntime(LOADER_THREAD);
  });

  it("serves a second run from disk rather than compiling again", () => {
    servesASecondRunFromDiskRatherThanCompilingAgain(LOADER_THREAD);
  });

  it("compiles again when uf itself was rebuilt", () => {
    compilesAgainWhenUfItselfWasRebuilt(LOADER_THREAD);
  });

  it("still has a build's entries when that build comes back", () => {
    stillHasABuildsEntriesWhenThatBuildComesBack(LOADER_THREAD);
  });

  it("does not serve what it cached when it cannot tell which uf compiled it", () => {
    doesNotServeWhatItCachedWhenItCannotTellWhichUfCompiledIt(LOADER_THREAD);
  });

  it("does not serve a build's entries to the build that replaced it mid-run", () => {
    doesNotServeABuildsEntriesToTheBuildThatReplacedItMidrun(LOADER_THREAD);
  });

  it("does not serve what it cached through a binary that can no longer run", () => {
    doesNotServeWhatItCachedThroughABinaryThatCanNoLongerRun(LOADER_THREAD);
  });

  it("identifies the binary a host started by hand finds on PATH", () => {
    identifiesTheBinaryAHostStartedByHandFindsOnPath(LOADER_THREAD);
  });

  it("fails the import of a module the compiler refused, with what the compiler said", () => {
    failsTheImportOfAModuleTheCompilerRefusedWithWhatTheCompilerSaid(LOADER_THREAD);
  });

  it("leaves a CommonJS module that something requires to Node", () => {
    leavesACommonjsModuleThatSomethingRequiresToNode(LOADER_THREAD);
  });
});
