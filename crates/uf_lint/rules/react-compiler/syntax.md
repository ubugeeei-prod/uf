Code the React Compiler rejects as invalid, even though the parser accepted it. Off by default: `flow/syntax` and `uf check` report most of what a module gets wrong.

## Bad

```js
// @flow
export component Counter() {
  const count = 0;
  count = 1;
  return <p>{count}</p>;
}
```

```diagnostics
app/example.js:4:3 Cannot reassign a `const` variable. `count` is declared as const.
```

## Good

```js
// @flow
export component Counter() {
  let count = 0;
  count = 1;
  return <p>{count}</p>;
}
```
