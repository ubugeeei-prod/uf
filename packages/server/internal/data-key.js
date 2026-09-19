// @flow

/** Canonical, bounded JSON arguments; no getters, coercions, or user serializers. */
export function dataKey(args: $ReadOnlyArray<mixed>): string {
  const open: Set<mixed> = new Set();
  let nodes = 0;
  function encode(value: mixed, depth: number): string {
    if (++nodes > 10000 || depth > 64) throw new TypeError("cacheFunction arguments are too large");
    if (value === null) return "null";
    if (value === undefined) return "undefined";
    if (typeof value === "string" || typeof value === "boolean") return JSON.stringify(value);
    if (typeof value === "number" && Number.isFinite(value)) {
      return Object.is(value, -0) ? "-0" : String(value);
    }
    if (typeof value !== "object") {
      throw new TypeError(`cacheFunction cannot key an argument of type ${typeof value}`);
    }
    const prototype = Object.getPrototypeOf(value);
    if (
      Array.isArray(value)
        ? prototype !== Object.getOwnPropertyDescriptor(Array, "prototype")?.value
        : prototype != null && prototype !== Object.getPrototypeOf({})
    ) {
      throw new TypeError("cacheFunction arguments must be plain JSON data");
    }
    if (open.has(value)) throw new TypeError("cacheFunction arguments contain a cycle");
    if (Object.getOwnPropertySymbols(value).length) {
      throw new TypeError("cacheFunction arguments cannot have symbol properties");
    }
    open.add(value);
    const descriptors = Object.getOwnPropertyDescriptors(value);
    if (Array.isArray(value)) {
      if (
        value.length > 10000 ||
        Object.keys(descriptors).some(
          (key) =>
            key !== "length" && (!/^(0|[1-9][0-9]*)$/.test(key) || Number(key) >= value.length),
        )
      )
        throw new TypeError("cacheFunction arrays must contain only bounded indexed data");
    }
    const keys = Array.isArray(value)
      ? Array.from({ length: value.length }, (_, index) => String(index))
      : Object.keys(descriptors).sort();
    const entries = keys.map((key) => {
      const descriptor = descriptors[key];
      if (descriptor == null) return "hole";
      if (descriptor.get != null || descriptor.set != null) {
        throw new TypeError(`cacheFunction argument property ${key} is an accessor`);
      }
      return `${JSON.stringify(key)}:${encode(descriptor.value, depth + 1)}`;
    });
    open.delete(value);
    return `${Array.isArray(value) ? "array" : "object"}{${entries.join(",")}}`;
  }
  const key = encode(args, 0);
  if (key.length > 65536) throw new TypeError("cacheFunction arguments exceed 64 KiB");
  return key;
}
