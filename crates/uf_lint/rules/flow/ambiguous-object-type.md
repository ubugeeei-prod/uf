An object type written as `{ id: string }` means exact or inexact depending on the project's `exact_by_default` setting, so the same annotation reads differently in two projects. Say which one you mean: `{| id: string |}` for exact, or `{ id: string, ... }` for inexact. Off by default, because Modern Flow makes objects exact by default.

## Bad

```js
// @flow
export type Props = { id: string };
```

```diagnostics
app/example.js:2:21 object type is neither exact (`{| |}`) nor explicitly inexact (`...`)
```

## Good

```js
// @flow
export type Exact = {| id: string |};
export type Open = { id: string, ... };
```
