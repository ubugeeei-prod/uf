// @flow
import * as React from "react";
import { App } from "./App.js";
test("keeps the existing component", () => { expect(React.isValidElement(App("Hello"))).toBe(true); });
