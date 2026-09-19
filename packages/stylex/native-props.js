// @flow

export type NativeStyleProps = { readonly style: { readonly [string]: string | number } };

/** Return null for web namespaces; mixing renderer outputs is an error. */
export function nativeProps(styles: $ReadOnlyArray<mixed>): NativeStyleProps | null {
  let native = false;
  let web = false;
  const merged: { [string]: string | number } = {};
  const pending = [...styles];
  for (let index = 0; index < pending.length; index += 1) {
    const value = pending[index];
    if (Array.isArray(value)) {
      pending.splice(index + 1, 0, ...value);
      continue;
    }
    if (value == null || typeof value !== "object") continue;
    // Static objects written by the compiler; validate their scalar fields.
    const namespace: { [string]: mixed } = value as $FlowFixMe;
    if (namespace.$$css === true) web = true;
    if (namespace.$$native !== true) continue;
    native = true;
    for (const property of Object.keys(namespace)) {
      if (property === "$$native") continue;
      const entry = namespace[property];
      if (typeof entry !== "string" && typeof entry !== "number") {
        throw new Error(`StyleX native: invalid compiled value for ${property}`);
      }
      merged[property] = entry;
    }
  }
  if (native && web) throw new Error("StyleX: cannot mix native style objects with web classes");
  return native ? { style: merged } : null;
}
