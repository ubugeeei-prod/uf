// @flow
//
// Internal to `@uniflowed/router`: reading the payload's rows out of a
// streaming document.
//
// `./payload.js` is the format and knows nothing about a browser. This is the
// other half: row 0 arrives in the document's `<script id="__uf_data">` and
// every later row arrives in a `<script data-uf-row="n">` React inserts when
// the value it holds resolves — which is *after* the parser reached the end of
// what had been sent, and usually after `hydrateRoot` has already been called.
// So a reader that only looked once would see the rows that happened to be
// early and wait forever for the rest.
//
// # A `MutationObserver`, and not an executable script
//
// The obvious mechanism is React's own: an inline `<script>` that pushes into
// a global. `internal/runtime.js` explains at length why uf's data element is
// `application/json` instead — "a script that is executed is a script a
// content security policy has to allow" — and the payload inherits the whole
// of that argument, several times over: there is now one element per deferred
// value rather than one per document, and each one carries application data
// that a `script` element with no type would hand to the JavaScript parser.
//
// The cost of keeping that property is that nothing calls uf when a row lands,
// so uf has to watch. A `MutationObserver` on the document, `childList` and
// `subtree`, is the whole of it. It is installed before `hydrateRoot` and
// disconnects itself the moment the last row the model named has arrived, so a
// page with no deferred values never has one and a page with three has one for
// as long as its slowest value takes.
//
// # Why the observer beats React to the row
//
// It has to, or a boundary that the server completed would be hydrated while
// this side still thought the value was pending. It does, for two reasons that
// are each sufficient. React writes a completed boundary's content into a
// hidden `<div>` *before* the inline script that moves it into place, so the
// row element is in the document one mutation earlier than React's own
// completion path; and a `MutationObserver` callback is a microtask, while
// React's hydration of a completed boundary is scheduled work in a later task.
//
// If it ever lost that race the failure is still bounded rather than wrong:
// `use` on a pending promise suspends, React keeps the boundary's fallback for
// one more microtask, and the row resolves it. What must not happen — and
// cannot, since both sides render the row element from the same value through
// the same `payloadJson` — is the two sides writing different bytes into it.
//
// # Rows nobody asked for
//
// Ignored. The ids that matter are the ones row 0 referred to, `resolve` is
// what records them, and an element carrying any other id is a document that
// says more than its model does. Refusing the page over it would be a
// hydration that fails because of something no component rendered; dropping it
// is the reading this module can defend.

import {
  type PayloadRowMessage,
  PAYLOAD_ROW_ATTRIBUTE,
  PayloadRowError,
  PayloadValueError,
  parseRowMessage,
} from "./payload.js";

/** The document half of a payload: the promises, and the watch that fills them. */
export type PayloadReader = {|
  /**
   * The value of row `id`, as a promise.
   *
   * The `RowResolver` `decodePayload` is handed. Calling it is what tells this
   * reader that the id is one the page is waiting for.
   */
  readonly resolve: (id: number) => Promise<mixed>,
  /**
   * Start reading. Applies every row already in the document, then watches for
   * the rest; returns without waiting for any of them.
   */
  readonly watch: () => void,
  /** Stop watching, whether or not every row arrived. */
  readonly stop: () => void,
|};

/** The parts of a `Document` this module uses, so it needs no DOM lib. */
type DocumentLike = interface {
  +querySelectorAll: (selector: string) => Iterable<ElementLike>,
  +documentElement: mixed,
};

/** The parts of an `Element` this module uses. */
type ElementLike = interface {
  +getAttribute: (name: string) => string | null,
  +textContent: string | null,
};

/** One row the page is waiting for. */
type Slot = {|
  readonly promise: Promise<mixed>,
  readonly settle: (message: PayloadRowMessage) => void,
  arrived: boolean,
|};

/**
 * A reader over one document.
 *
 * `observe` is passed in rather than reached for so that this module needs no
 * `MutationObserver` global to be *loaded* — the tests drive it with a
 * document they mutate by hand, and a runtime without the constructor gets a
 * reader that reads what is already there and never watches, which is exactly
 * what a prerendered document needs.
 */
export function createPayloadReader(
  document: DocumentLike,
  observe?: ?(callback: () => void) => (() => void) | null,
): PayloadReader {
  const slots: Map<number, Slot> = new Map();
  let disconnect: (() => void) | null = null;
  let watching = false;

  function slotFor(id: number): Slot {
    const existing = slots.get(id);
    if (existing != null) {
      return existing;
    }
    let settle: (message: PayloadRowMessage) => void = () => {};
    const promise = new Promise<mixed>((fulfil, reject) => {
      settle = (message) => {
        if (message.error !== undefined) {
          // A `PayloadRowError` rather than a plain one, carrying the row's
          // exact text: the page's error boundary sees an `Error` either way,
          // and `internal/runtime.js` needs the text back verbatim when it
          // re-renders this row's element. See `PayloadRowError`.
          reject(new PayloadRowError(message.error));
          return;
        }
        fulfil(message.value);
      };
    });
    // Marked handled the moment it exists. A row that says the value failed
    // rejects this promise, and whether anything is listening by then depends
    // on whether React has rendered the row's element yet — so without this an
    // ordinary loader failure would arrive as an unhandled rejection, which
    // recent Node turns into an exit.
    promise.catch(() => {});
    const slot: Slot = { promise, settle, arrived: false };
    slots.set(id, slot);
    return slot;
  }

  function apply(element: ElementLike): void {
    const attribute = element.getAttribute(PAYLOAD_ROW_ATTRIBUTE);
    if (attribute == null) {
      return;
    }
    const id = Number.parseInt(attribute, 10);
    if (!Number.isSafeInteger(id)) {
      return;
    }
    const slot = slots.get(id);
    // A row for an id the model never referred to, or one already applied. See
    // the header: neither is this reader's to complain about.
    if (slot == null || slot.arrived) {
      return;
    }
    slot.arrived = true;
    try {
      slot.settle(parseRowMessage(element.textContent ?? "", id));
    } catch (error) {
      slot.settle({
        error:
          error instanceof PayloadValueError
            ? error.message
            : `@uniflowed/router: row ${String(id)} could not be read.`,
      });
    }
    if (finished()) {
      stop();
    }
  }

  function sweep(): void {
    for (const element of document.querySelectorAll(`script[${PAYLOAD_ROW_ATTRIBUTE}]`)) {
      apply(element);
    }
  }

  function finished(): boolean {
    for (const slot of slots.values()) {
      if (!slot.arrived) {
        return false;
      }
    }
    return true;
  }

  function stop(): void {
    watching = false;
    const off = disconnect;
    disconnect = null;
    if (off != null) {
      off();
    }
  }

  return {
    resolve(id: number): Promise<mixed> {
      const slot = slotFor(id);
      // A reference discovered after the watch started — a client navigation
      // decoding a payload of its own — still gets whatever is already in the
      // document, so the order the two calls happen in does not matter.
      if (watching) {
        sweep();
      }
      return slot.promise;
    },
    watch(): void {
      if (watching || slots.size === 0) {
        return;
      }
      watching = true;
      sweep();
      if (finished() || observe == null) {
        watching = false;
        return;
      }
      disconnect = observe(sweep);
    },
    stop,
  };
}

/**
 * The `observe` a browser has.
 *
 * Separate from [`createPayloadReader`] so the reader itself stays testable
 * without one, and so the `MutationObserver` global is touched in exactly one
 * place — a runtime that has no such constructor gets `null` and a reader that
 * reads the document once, which is the right answer for a prerendered file
 * where every row is already in it.
 */
export function domObserver(
  document: DocumentLike,
): ((callback: () => void) => (() => void) | null) | null {
  if (typeof MutationObserver === "undefined") {
    return null;
  }
  const root = document.documentElement;
  if (root == null) {
    return null;
  }
  return (callback) => {
    const observer = new MutationObserver(() => {
      callback();
    });
    // $FlowFixMe[incompatible-call] `documentElement` is a `Node`; the interface above says only what is read.
    observer.observe(root, { childList: true, subtree: true });
    return () => {
      observer.disconnect();
    };
  };
}
