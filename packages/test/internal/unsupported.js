// @flow
//
// Internal to `@uniflowed/test`: the error a `uft` binding raises when this
// host cannot give it.
//
// Its own module because two of them need it — the namespace, and the module
// mocking in `./modules.js` that the namespace imports — and a class that both
// of a pair of modules reaches for is a third module, not an import cycle.
//
// The class is exported from the package, so a test can catch it by name
// rather than by matching on a message.

/**
 * Raised for a `uft` binding this host cannot provide.
 *
 * Names what the binding needs rather than only that it is missing: a reader
 * who is told that module interception needs synchronous module hooks can
 * decide whether to run the suite on another host or to restructure the test,
 * and neither is a decision a bare "not implemented" supports.
 */
export class UnsupportedError extends Error {
  /** The binding that was called. */
  binding: string;

  constructor(binding: string, reason: string) {
    super(`uft.${binding} is not available: ${reason}`);
    this.name = "UnsupportedError";
    this.binding = binding;
  }
}
