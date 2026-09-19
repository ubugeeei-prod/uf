// @flow
import type { RouteTable } from "./routing.js";
import { hasClientPage, matchRoute } from "./routing.js";

type Table = RouteTable<mixed, mixed, mixed, mixed, mixed>;

/** One parser for cold starts, warm deliveries and programmatic navigation. */
export function nativeLinkHref(table: Table, to: string): string | null {
  const links = table.nativeLinks;
  if (links == null || !/^https:\/\//i.test(to) || /[\\\u0000-\u0020]/.test(to)) return null;
  try {
    const url = new URL(to);
    if (url.username !== "" || url.password !== "" || url.hash !== "") return null;
    // URL does not reject malformed percent escapes. Encoded separators must
    // not change the route when another layer decodes the path a second time.
    decodeURIComponent(url.pathname + url.search);
    if (/%(?:2f|5c)/i.test(url.pathname)) return null;
    if (!links.origins.some((origin) => new URL(origin).origin === url.origin)) return null;
    const match = matchRoute(table.routes, url.pathname);
    if (match == null || !hasClientPage(match.route) || !links.routes.includes(match.route.path))
      return null;
    return url.pathname + url.search;
  } catch {
    return null;
  }
}

export type NativeLinkSource = {|
  readonly getInitialURL: () => Promise<string | null>,
  readonly addEventListener: (
    event: "url",
    listener: (event: {| url: string |}) => void,
  ) => {| remove: () => void |},
|};

/** Pass React Native's Linking; rejected links stay with the OS/browser. */
export function createNativeLinking(
  table: Table,
  source: NativeLinkSource,
): {|
  getInitialURL: () => Promise<string | null>,
  subscribe: (listener: (href: string) => void) => () => void,
|} {
  let last: string | null = null;
  let deliveredAt = -Infinity;
  function receive(url: string): string | null {
    const href = nativeLinkHref(table, url);
    const now = Date.now();
    if (href == null || (href === last && now - deliveredAt < 1000)) return null;
    last = href;
    deliveredAt = now;
    return href;
  }
  return {
    async getInitialURL() {
      const url = await source.getInitialURL();
      return url == null ? null : receive(url);
    },
    subscribe(listener) {
      const subscription = source.addEventListener("url", ({ url }) => {
        const href = receive(url);
        if (href != null) listener(href);
      });
      return () => subscription.remove();
    },
  };
}
