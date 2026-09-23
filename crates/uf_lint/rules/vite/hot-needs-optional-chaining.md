`import.meta.hot` is `undefined` outside the dev server. The usual Vite guard, `if (import.meta.hot)`, does not refine it for Flow, because `import.meta.hot` is not a local variable, so `uf check` still reports the call inside. Reach it through `?.`, or copy it into a local variable first.

## Bad

```js
// @flow
if (import.meta.hot) {
  import.meta.hot.accept();
}
```

```diagnostics
app/example.js:3:3 `import.meta.hot` is undefined in a build, and `if (import.meta.hot)` does not refine it: Flow cannot key a refinement on `import.meta`. Write `import.meta.hot.accept` with `?.`, or bind `const hot = import.meta.hot;` first and test that
```

## Good

```js
// @flow
import.meta.hot?.accept();

const hot = import.meta.hot;
if (hot) {
  hot.dispose(() => {});
}
```
