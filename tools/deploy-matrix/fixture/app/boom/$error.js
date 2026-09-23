"use client";
// @flow
//
// The error boundary for `/boom`: what a page that throws while rendering is
// answered with, inside the layouts above it, with a `500`.
//
// A client module, because an error boundary also catches a throw while the
// browser renders; the router refuses a boundary without the directive — at
// request time, which is when a page first throws.

import * as React from "@uniflowed/react";
import type { RouteError } from "@uniflowed/router";

export default component Boom(error: RouteError) {
  return <h1>{`boom boundary: ${error.kind}`}</h1>;
}
