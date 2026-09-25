// @flow
//
// A page's typed search params, and the test that says the prop is the
// schema's output.
//
// Every refusal in this file is a type error `uf check` must raise, suppressed
// where it stands; `tests/library/type-tests.js` says how a marker is read.
// `@uniflowed/router` claims that a page exporting
//
//     export const searchParams = object({ … });
//
// can type its `searchParams` prop as `SearchParamsOf<typeof searchParams>` and
// get exactly what the schema produces — numbers where the schema coerces to a
// number, a list where it says array — so the prop and the export cannot drift
// apart. That is a claim about types and is proved by the checker:
// `packages/router/search-params.test.js` runs this file with the router and
// the validator beside it, for the reason `field-paths.js` gives.

import * as React from "@uniflowed/react";

import type { PageProps, SearchParamsOf } from "../../packages/router/index.js";
import type { Schema } from "../../packages/validator/index.js";
import {
  array,
  number,
  object,
  optional,
  pipe,
  string,
  transform,
} from "../../packages/validator/index.js";

export const searchParams: Schema<{
  page: number,
  tag: $ReadOnlyArray<string>,
  q?: string,
  sort: boolean,
}> = object({
  page: number(),
  tag: array(string()),
  q: optional(string()),
  sort: pipe(
    string(),
    transform((value: string) => value === "new"),
  ),
});

type Props = PageProps<{}, void, SearchParamsOf<typeof searchParams>>;

export component Page(...props: Props) {
  // What the schema produces, read as what it is.
  const page: number = props.searchParams.page;
  const tags: $ReadOnlyArray<string> = props.searchParams.tag;
  const query: ?string = props.searchParams.q;
  // The output of the transform, not the string the query carried.
  const newest: boolean = props.searchParams.sort;

  // $FlowExpectedError[incompatible-type] number is incompatible with string
  const pageAsText: string = props.searchParams.page;

  // $FlowExpectedError[incompatible-type] a list of tags is not one tag
  const oneTag: string = props.searchParams.tag;

  // $FlowExpectedError[incompatible-type] the transform made a boolean of it
  const sortText: string = props.searchParams.sort;

  // $FlowExpectedError[prop-missing] the schema names no `limit`
  const limit = props.searchParams.limit;

  return (
    <p>
      {page} {tags.join(",")} {query} {String(newest)} {pageAsText} {oneTag} {sortText}{" "}
      {String(limit)}
    </p>
  );
}

// And a page with no schema is given the string map it always was.
export component Plain(...props: PageProps<>) {
  const q: ?string = props.searchParams.q;

  // $FlowExpectedError[incompatible-type] every value of the plain map is a string
  const count: number = props.searchParams.count;

  return (
    <p>
      {q} {count}
    </p>
  );
}
