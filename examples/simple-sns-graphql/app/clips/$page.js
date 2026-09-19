// @flow
import type { SearchParams } from "@uniflowed/router";
import { renderScreen } from "../screen.server.js";
export const dynamic = "force-dynamic";
export component Page(searchParams: SearchParams) {
  return renderScreen("clips", searchParams);
}
