An existence check such as `if (value)` on a `?string` is false both when the value is missing and when it is `""`, and the code usually meant only one of them. Compare with `null` explicitly. Off by default: `flow/sketchy-null` covers it.

## Bad

```js
// @flow
export function describe(value: ?string): string {
  if (value) {
    return "set";
  }
  return "missing";
}
```

## Good

```js
// @flow
export function describe(value: ?string): string {
  if (value != null) {
    return "set";
  }
  return "missing";
}
```
