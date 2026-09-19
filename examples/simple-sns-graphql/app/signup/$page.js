// @flow
import type { LoaderArgs } from "@uniflowed/router";
import { renderScreen } from "../screen.server.js";
export const dynamic = "force-dynamic";
export function loader({ searchParams }: LoaderArgs) {
  return searchParams;
}
export component Page(data: { readonly [string]: mixed }) {
  return renderScreen("signup", data);
}
