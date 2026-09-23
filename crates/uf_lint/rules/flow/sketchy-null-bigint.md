An existence check such as `if (value)` on a `?bigint` is false both when the value is missing and when it is `0n`, and the code usually meant only one of them. Compare with `null` explicitly. Off by default: `flow/sketchy-null` covers it.

## Bad

```js
// @flow
export function describe(value: ?bigint): string {
  if (value) {
    return "set";
  }
  return "missing";
}
```

## Good

```js
// @flow
export function describe(value: ?bigint): string {
  if (value != null) {
    return "set";
  }
  return "missing";
}
```
