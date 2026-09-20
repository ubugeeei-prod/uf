"use client";
// @flow
import * as React from "@uniflowed/react";
import { failAction } from "./action.js";

export default component Button() {
  return <button onClick={() => failAction()}>fail action</button>;
}
