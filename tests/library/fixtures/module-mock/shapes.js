// @flow
//
// One of each kind of export, so the automatic stand-in's rules can be
// asserted rather than assumed.

/** A function, which becomes a spy. */
export function greet(name: string): string {
  return `hello ${name}`;
}

/** A class, which stays `new`-able with its methods replaced. */
export class Session {
  token: string;

  constructor(token: string) {
    this.token = token;
  }

  /** A method, which becomes a spy on the stand-in's prototype. */
  identify(): string {
    return `session ${this.token}`;
  }
}

/** An array, which is emptied. */
export const ROLES: $ReadOnlyArray<string> = ["admin", "reader"];

/** A number, which is kept: there is nothing in it to call. */
export const RETRIES: number = 3;

/** A nested object, which is followed key by key. */
export const config: { readonly name: string, readonly load: () => string } = {
  name: "production",
  load: () => "loaded",
};
