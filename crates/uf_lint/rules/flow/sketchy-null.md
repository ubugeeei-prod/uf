An existence check such as `if (value)` on a `?number` is false both when the value is missing and when it is `0`, and the code usually meant only one of them. Compare with `null` explicitly. This rule covers every such type; the `sketchy-null-*` rules name one type each.

## Bad

```js
// @flow
export function describe(value: ?number): string {
  if (value) {
    return "set";
  }
  return "missing";
}
```

## Good

```js
// @flow
export function describe(value: ?number): string {
  if (value != null) {
    return "set";
  }
  return "missing";
}
```
