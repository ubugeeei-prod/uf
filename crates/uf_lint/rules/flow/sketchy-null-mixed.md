An existence check such as `if (value)` on a `mixed` value is false for `null` and `undefined`, and also for `0`, `""` and `false`, which the code usually did not mean to reject. Compare with `null` explicitly. Off by default: `flow/sketchy-null` covers it.

## Bad

```js
// @flow
export function present(value: mixed): boolean {
  if (value) {
    return true;
  }
  return false;
}
```

## Good

```js
// @flow
export function present(value: mixed): boolean {
  return value != null;
}
```
