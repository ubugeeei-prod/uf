// @flow
import { it } from "@uniflowed/test";
import { everyMisuseIsReported } from "./type-tests.js";
it("rejects invalid Commonplace ADTs and composition slots", () => {
  everyMisuseIsReported({
    fixture: "tests/type-tests/simple-sns.js",
    alongside: ["examples/simple-sns"],
    atLeast: 5,
  });
});
