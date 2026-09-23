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
app/example.js:6:1 export the component by name: the router reads a named `Page`, `Layout` and the rest as well as `default`, and a named component keeps one name in every import, stack trace and DevTools tree
```

## Good

```js
// @flow
export component Settings() {
  return <h1>Settings</h1>;
}
```
