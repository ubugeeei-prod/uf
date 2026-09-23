Children belong between the tags. Passed as a `children` prop they are easy to miss when reading, and anything nested between the tags silently replaces them.

## Bad

```js
// @flow
export component Notice(text: string) {
  return <p children={text} />;
}
```

```diagnostics
app/example.js:3:13 pass `children` to `<p>` between its tags rather than as a prop, which is where a reader of JSX looks for them
```

## Good

```js
// @flow
export component Notice(text: string) {
  return <p>{text}</p>;
}
```
