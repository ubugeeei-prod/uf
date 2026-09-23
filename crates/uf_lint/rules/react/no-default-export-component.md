A named export keeps a component's name the same at every import site, so it can be searched for, renamed by tools and read in stack traces. uf's router reads a page module's `default` export or its named `Page`, so a named export works for routes too.

## Bad

```js
// @flow
component Settings() {
  return <h1>Settings</h1>;
}

export default Settings;
```

```diagnostics
app/example.js:6:1 framework routes are wired by name; export components with a named export
```

## Good

```js
// @flow
export component Settings() {
  return <h1>Settings</h1>;
}
```
