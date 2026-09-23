A prop that a `component` declares and never reads is part of its type that does nothing. Callers keep passing it for no effect. Remove it, or use it.

## Bad

```js
// @flow
export component Badge(label: string, tone: "info" | "warning") {
  return <span>{label}</span>;
}
```

```diagnostics
app/example.js:2:39 `<Badge>` declares the prop `tone` and never reads it, so every caller is asked for a value that goes nowhere; read it, or drop it from the parameter list
```

## Good

```js
// @flow
export component Badge(label: string, tone: "info" | "warning") {
  return <span data-tone={tone}>{label}</span>;
}
```
