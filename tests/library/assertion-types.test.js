// @flow
//
// What `uf check` says about the two surfaces that used to be `any`.
//
// `@uniflowed/test`'s `expect` and `@uniflowed/react-testing`'s `fireEvent` are
// the same problem twice: a value that answers to a name decided while the
// program runs, published from packages whose entire purpose is testing *typed*
// code, and typed as `any` because nothing else could be assigned to what they
// were. So `expect(user).toBaa(1)` and `fireEvent.clcik(button)` were calls
// nobody's checker refused — and neither fails at run time either, because a
// matcher that does not exist asserts nothing and an event nobody listens for
// changes nothing. They pass, and they read in review as tests.
//
// Both are written-out listings now, and the claim a listing makes is not
// provable by running anything: no assertion about behaviour can say that a
// *different* program would have been rejected. So it is proved the only way it
// can be — by running the checker over code that must fail and reading what it
// said.
//
// `tests/type-tests/matchers.js` and `tests/type-tests/event-names.js` are
// those misuses, written down. Each marks the lines that must be reported with
// a `// expect:` comment, and this reads both halves: a marked line that stops
// being an error fails here, and so does an unmarked line that starts being
// one.

import path from "node:path";
import { describe, it } from "@uniflowed/test";

import { everyMisuseIsReported } from "./type-tests.js";

describe("a matcher is a name the checker knows", () => {
  // `expect` was `$FlowFixMe`, and so was everything it handed back, so a
  // misspelt matcher and an argument of the wrong type were both silent. Every
  // test in this repository is written against it.
  //
  // `tests/type-tests/matchers.js` is the misuse, written down. Its tail is
  // the other half of the claim: four correct assertions that a `toBe` carrying
  // the received value's type would refuse, which is why it does not carry one.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "matchers.js"),
      alongside: ["packages/test", "packages/react-testing"],
      atLeast: 6,
    });
  });
});

describe("an event name is a name the checker knows", () => {
  // The same claim for `fireEvent`, which was a `Proxy` and so could carry no
  // type at all. Two of the fixture's markers are the price of the table
  // rather than a bug in it — an unlisted name and the DOM's lower-case
  // spelling both stop working — and they are marked so that the loss stays a
  // decision somebody made.
  //
  // `tests/type-tests/event-names.js` is the misuse, written down.

  it("reports every misuse, and only the misuses", () => {
    everyMisuseIsReported({
      fixture: path.join("tests", "type-tests", "event-names.js"),
      alongside: ["packages/test", "packages/react-testing"],
      atLeast: 2,
    });
  });
});
