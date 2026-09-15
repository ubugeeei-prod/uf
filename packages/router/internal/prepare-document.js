// @flow
//
// Internal to `@uniflowed/router`: a server's document, made ready for React to
// hydrate.
//
// Where a server wrote the head's metadata, an element React leaves behind, and
// the placeholder action React writes into a form whose action is a function
// (its server and its client spell that placeholder differently): none of these
// comes from a route, and each would be reported as a mismatch. So each is put
// right before `hydrateRoot` compares the markup with the tree. Both ways of
// hydrating do it — `hydrate` in `../client.js`, for a route rendered from its
// modules, and `hydrateFlight` in `../rsc-client.js`, for one React Server
// Components rendered — so the code lives here rather than in either entry.

export function prepareDocumentForHydration(document: Document): void {
  const head = document.head;
  const envelope = head.querySelector('meta[name="uf:render"]');
  if (envelope != null && head.firstChild !== envelope) {
    head.insertBefore(envelope, head.firstChild);
  }
  moveLayoutMetaAfterRouteHead(head, head.querySelector("meta[charset]"));
  moveLayoutMetaAfterRouteHead(head, head.querySelector('meta[name="viewport"]'));
  document.getElementById("_R_")?.remove();
  normalizeReactFormActions(document);
}

function moveLayoutMetaAfterRouteHead(head: HTMLHeadElement, meta: Element | null): void {
  if (meta == null) {
    return;
  }
  const colorScheme = head.querySelector('meta[name="color-scheme"]');
  if (colorScheme != null && colorScheme !== meta) {
    head.insertBefore(meta, colorScheme);
    return;
  }
  head.appendChild(meta);
}

const SERVER_FORM_PLACEHOLDER = "javascript:throw new Error('React form unexpectedly submitted.')";
const CLIENT_FORM_PLACEHOLDER =
  "javascript:throw new Error('A React form was unexpectedly submitted. If you called form.submit() manually, consider using form.requestSubmit() instead. If you\\'re trying to use event.stopPropagation() in a submit event handler, consider also calling event.preventDefault().')";

function normalizeReactFormActions(document: Document): void {
  for (const form of document.querySelectorAll("form")) {
    if (form.getAttribute("action") === SERVER_FORM_PLACEHOLDER) {
      form.setAttribute("action", CLIENT_FORM_PLACEHOLDER);
    }
  }
}
