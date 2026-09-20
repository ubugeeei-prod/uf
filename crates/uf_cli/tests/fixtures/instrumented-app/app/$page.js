// @flow
import * as React from "@uniflowed/react";
import { client } from "./_shared/fetch.js";

export default async function Page() {
  await client.raw("https://upstream.example/");
  return <main>instrumented RSC page</main>;
}
