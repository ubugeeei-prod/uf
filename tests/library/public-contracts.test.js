// @flow
// These contracts need a checker: runtime tests cannot prove a bad call fails.

import { describe, it } from "@uniflowed/test";
import { everyMisuseIsReported } from "./type-tests.js";

describe("the public query, StyleX and byte I/O type contracts", () => {
  it("infers correct uses and rejects each misuse", () => {
    everyMisuseIsReported({
      fixture: "tests/type-tests/public-contracts.js",
      atLeast: 4,
      alongside: ["packages/query", "packages/stylex", "packages/std"],
    });
  });
});
