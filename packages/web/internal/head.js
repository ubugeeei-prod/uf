// @flow
//
// Internal to `@uniflowed/web`: the document head, from a component.
//
// A page's title and its meta tags are decided by whatever is rendering, which
// is usually not the thing that owns `<head>`. `useHead` lets a component say
// what it needs and puts it there.
//
// # What this does and does not do
//
// In a browser it applies. On a server it does not, and that is deliberate
// rather than unfinished: the head is written before the body is streamed, so a
// component that renders *after* the head has gone cannot change it. uf's
// answer for server-rendered metadata is the route's own `metadata` export,
// which the router resolves before it renders anything — `useHead` is for what
// a page decides while it is running.
//
// Saying so matters because the alternative is a silent one. A `useHead` that
// pretended to work on a server would produce a page whose title is right in a
// browser and wrong in a crawler, which is the case nobody checks.

import * as React from "@uniflowed/react";

/** One `<meta>`, by whichever attribute names it. */
export type Meta = {
  readonly name?: string,
  readonly property?: string,
  readonly content: string,
};

/** One `<link>`. */
export type Link = {
  readonly rel: string,
  readonly href: string,
  readonly type?: string,
  readonly sizes?: string,
};

/** What a component wants in the head. */
export type Head = {
  readonly title?: string,
  readonly meta?: $ReadOnlyArray<Meta>,
  readonly links?: $ReadOnlyArray<Link>,
};

/** Marks the elements this module owns, so it can remove its own and no others. */
const OWNED = "data-uf-head";

/** The attribute that names one `<meta>`, and the value that identifies it. */
function metaKey(meta: Meta): string {
  return meta.name != null ? `name=${meta.name}` : `property=${meta.property ?? ""}`;
}

/**
 * Put `head` in the document, and take it out again when the component goes.
 *
 * Every element it adds is marked, and only marked elements are removed — a
 * component that unmounts must not take the document's own `<meta charset>`
 * with it.
 *
 * The title is restored rather than removed, because a document with no title
 * shows its URL in the tab, which looks like a bug.
 */
export function useHead(head: Head): void {
  // The whole head as a string: an object literal is a new object every render,
  // and depending on it would re-apply on every one.
  const key = JSON.stringify(head);

  React.useEffect(() => {
    const document = (globalThis: $FlowFixMe).document;
    if (document == null) {
      return undefined;
    }

    const previousTitle = document.title;
    if (head.title != null) {
      document.title = head.title;
    }

    const added: Array<mixed> = [];
    for (const meta of head.meta ?? []) {
      const element = document.createElement("meta");
      if (meta.name != null) {
        element.setAttribute("name", meta.name);
      }
      if (meta.property != null) {
        element.setAttribute("property", meta.property);
      }
      element.setAttribute("content", meta.content);
      element.setAttribute(OWNED, metaKey(meta));
      document.head.appendChild(element);
      added.push(element);
    }
    for (const link of head.links ?? []) {
      const element = document.createElement("link");
      element.setAttribute("rel", link.rel);
      element.setAttribute("href", link.href);
      if (link.type != null) {
        element.setAttribute("type", link.type);
      }
      if (link.sizes != null) {
        element.setAttribute("sizes", link.sizes);
      }
      element.setAttribute(OWNED, `${link.rel}:${link.href}`);
      document.head.appendChild(element);
      added.push(element);
    }

    return () => {
      if (head.title != null) {
        document.title = previousTitle;
      }
      for (const element of added) {
        (element: $FlowFixMe).remove();
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);
}
