// @flow

import { buildRoute } from "@uniflowed/router/routing";

export type RoutePath = "/" | "/clips" | "/login" | "/messages" | "/settings" | "/signup";

export type RouteParams = {
  "/": {},
  "/clips": {},
  "/login": {},
  "/messages": {},
  "/settings": {},
  "/signup": {},
};

export type RouteArgs = {
  "/": [],
  "/clips": [],
  "/login": [],
  "/messages": [],
  "/settings": [],
  "/signup": [],
};

export function route<Path extends RoutePath>(path: Path, ...params: RouteArgs[Path]): string {
  return buildRoute(path, ...params);
}
