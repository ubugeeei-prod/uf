`bool` is a deprecated alias Flow still accepts. Write `boolean`, the name Flow and every other type system use.

## Bad

```js
// @flow
export type Toggle = { readonly on: bool };
```

```diagnostics
app/example.js:2:37 the `bool` type alias is deprecated; write `boolean`
```

## Good

```js
// @flow
export type Toggle = { readonly on: boolean };
```
