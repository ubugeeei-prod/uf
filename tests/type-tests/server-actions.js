// @flow
//
// Server actions misused, and the test that says the checker catches it.
//
// This file is *supposed* to fail `uf check`. Every marked line is a mistake
// somebody can make about a `"use server"` function, and the claim being
// proved is that each is caught before a request is ever made — because a
// server action is the one place in an application where a wrong argument
// crosses a network, and the answer on the other side is a `400` with nothing
// in it.
//
// # How it is read
//
// A `// expect:` comment says that the line after it must be reported, and
// that the report must contain that text. A line without one must not be
// reported at all, so a change that makes any of these *stop* being an error
// fails the test, and so does one that makes something else here start being
// one. `tests/library/server-actions.test.js` runs it.
//
// # The two kinds of mistake, and why one needs the library at all
//
// The first is calling an action wrongly, and nothing here makes it an error:
// Flow reads `createUser`'s declaration whether the caller is on the server or
// in the browser, because the reference `@uniflowed/vite` substitutes for the
// module is a bundler artefact and not something the checker sees. That half
// works because uf never let it stop working, and it is written down here so
// that a change which breaks it is caught by a test rather than by a user.
//
// The second is an action whose arguments or result cannot cross a wire at
// all. `createUser(when: Date)` type-checks perfectly at every call site and
// then arrives on the server as `{}`, because a `Date` has no JSON spelling —
// so `uf prepare` writes one `ActionArguments` and one `ActionResult` into the
// generated `server-actions.js`, over every action in the project at once. The
// instantiations below are what that file contains, with a project's real
// action table replaced by two written here.

import type { ActionArguments, ActionResult, ActionValue } from "../../packages/router/action.js";

// A `"use server"` module, as Flow reads it. Ordinary declarations: what makes
// them actions is a directive the checker does not need to know about.
declare function createUser(name: string, age: number): Promise<{ readonly id: string }>;
declare function clearUsers(): Promise<void>;

// And two an application should not be able to write: one that takes a value
// no payload can carry, one that answers with a value no payload can carry.
declare function watchUsers(onChange: () => void): Promise<null>;
declare function usersByName(): Promise<Map<string, number>>;

// And one the RSC graph rejects for a different reason, kept here because the
// wire says the same thing about it: React's calling convention makes a
// synchronous `"use server"` export a correctness *and* a safety issue, and a
// result that is not a promise cannot be one an endpoint awaited.
declare function userCount(): number;

type Actions = {
  "app/_actions/users.js#createUser": typeof createUser,
  "app/_actions/users.js#clearUsers": typeof clearUsers,
};
type UnwireableActions = {
  "app/_actions/users.js#watchUsers": typeof watchUsers,
  "app/_actions/users.js#usersByName": typeof usersByName,
};

type SynchronousActions = { "app/_actions/users.js#userCount": typeof userCount };

type Name = $Keys<Actions>;
type UnwireableName = $Keys<UnwireableActions>;
type SynchronousName = $Keys<SynchronousActions>;

// --- Calling an action wrongly, which is what a client component does ---

// expect: 36 is incompatible with string
export const wrongArgumentType: Promise<{ readonly id: string }> = createUser(36, 36);

// expect: Cannot call createUser because function requires another argument
export const tooFewArguments: Promise<{ readonly id: string }> = createUser("ada");

// expect: {readonly id: string} is incompatible with string
export const wrongResultType: Promise<string> = createUser("ada", 36);

// --- An action whose signature cannot cross the wire ---
//
// These two are what `server-actions.js` holds a whole project against.

// expect: Cannot instantiate ActionArguments
export type WatchArgsFitTheWire = ActionArguments<Parameters<UnwireableActions[UnwireableName]>>;

// expect: Map<string, number> is incompatible with
export type WatchResultsFitTheWire = ActionResult<ReturnType<UnwireableActions[UnwireableName]>>;

// expect: Cannot instantiate ActionResult
export type SyncFitsTheWire = ActionResult<ReturnType<SynchronousActions[SynchronousName]>>;

// --- And what must stay usable ---

export const rightCall: Promise<{ readonly id: string }> = createUser("ada", 36);
export const noArguments: Promise<void> = clearUsers();
export type ArgsFitTheWire = ActionArguments<Parameters<Actions[Name]>>;
export type ResultsFitTheWire = ActionResult<ReturnType<Actions[Name]>>;
export const aValue: ActionValue = { rows: [1, "two", null, true], nested: { deep: [] } };
