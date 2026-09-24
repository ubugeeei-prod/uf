// @flow
import * as React from "@uniflowed/react";
import { cookies } from "@uniflowed/server";
import { Counter } from "./_components/Counter.js";
// Reads the visitor cookie, so it is rendered for each request rather than prerendered.
export const dynamic = "force-dynamic";
async function Delayed() {
  await new Promise((resolve) => setTimeout(resolve, 80));
  return <p id="resolved">nested async data</p>;
}
export async function Page({ searchParams }: { searchParams: { [string]: string, ... } }) {
  const response = await fetch("data:application/json," + encodeURIComponent(JSON.stringify({ name: searchParams.name ?? "Ada" })));
  const person = await response.json();
  const visitor = cookies().get("visitor") ?? "guest";
  return <section><h1>{person.name}</h1><p id="visitor">{visitor}</p>
    <React.Suspense fallback={<p id="pending">pending data</p>}><Delayed /></React.Suspense>
    <Counter />
  </section>;
}
