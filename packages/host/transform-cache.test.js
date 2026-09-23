// @flow
//
// The transform cache, through the in-thread hooks `register.js` installs
// wherever `node:module` has `registerHooks` — which is every Node this suite
// runs on.
//
// The bodies are `../../tests/library/transform-cache.js`, shared with
// `./transform-cache-loader-thread.test.js`; the names are here because `uf test` reads this file's own
// source to decide whether it holds tests at all.

import { describe, it } from "@uniflowed/test";

import {
  IN_THREAD_HOOKS,
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
  leavesWhatUfBuildWroteToTheHost,
} from "../../tests/library/transform-cache.js";

describe("the transform cache, through the in-thread hooks", () => {
  it("uses an installed compiler runtime before the standalone test fallback", () => {
    resolvesTheProjectsCompilerRuntime(IN_THREAD_HOOKS);
  });

  it("serves a second run from disk rather than compiling again", () => {
    servesASecondRunFromDiskRatherThanCompilingAgain(IN_THREAD_HOOKS);
  });

  it("compiles again when uf itself was rebuilt", () => {
    compilesAgainWhenUfItselfWasRebuilt(IN_THREAD_HOOKS);
  });

  it("still has a build's entries when that build comes back", () => {
    stillHasABuildsEntriesWhenThatBuildComesBack(IN_THREAD_HOOKS);
  });

  it("does not serve what it cached when it cannot tell which uf compiled it", () => {
    doesNotServeWhatItCachedWhenItCannotTellWhichUfCompiledIt(IN_THREAD_HOOKS);
  });

  it("does not serve a build's entries to the build that replaced it mid-run", () => {
    doesNotServeABuildsEntriesToTheBuildThatReplacedItMidrun(IN_THREAD_HOOKS);
  });

  it("does not serve what it cached through a binary that can no longer run", () => {
    doesNotServeWhatItCachedThroughABinaryThatCanNoLongerRun(IN_THREAD_HOOKS);
  });

  it("identifies the binary a host started by hand finds on PATH", () => {
    identifiesTheBinaryAHostStartedByHandFindsOnPath(IN_THREAD_HOOKS);
  });

  it("fails the import of a module the compiler refused, with what the compiler said", () => {
    failsTheImportOfAModuleTheCompilerRefusedWithWhatTheCompilerSaid(IN_THREAD_HOOKS);
  });

  it("leaves a CommonJS module that something requires to Node", () => {
    leavesACommonjsModuleThatSomethingRequiresToNode(IN_THREAD_HOOKS);
  });

  it("leaves what uf build wrote to the host, as the ES module it is", () => {
    leavesWhatUfBuildWroteToTheHost(IN_THREAD_HOOKS);
  });
});
