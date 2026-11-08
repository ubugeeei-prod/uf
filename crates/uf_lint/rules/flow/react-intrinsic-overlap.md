A value used as a JSX component whose type overlaps an intrinsic element's name, such as a string `"div"`, renders a DOM element where you meant a component. Give JSX a component, not a string that may be a tag.

## Bad

```js
"use flow";
export component Box(as: string) {
  const Tag = as;
  return <Tag />;
}
```

## Good

```js
"use flow";
export component Box(as: "section" | "article") {
  return as === "section" ? <section /> : <article />;
}
```
