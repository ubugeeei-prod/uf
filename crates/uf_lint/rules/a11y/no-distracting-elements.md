`marquee` and `blink` move or flash with no way to pause them, which makes the page hard to read and can be harmful (WCAG 2.2.2).

## Bad

```js
// @flow
export component Banner() {
  return (
    <marquee>Sale ends tonight</marquee>
  );
}
```

```diagnostics
app/example.js:4:5 `<marquee>` animates on its own and gives the reader no way to stop it, which WCAG 2.2.2 requires for anything that moves for more than five seconds; HTML removed the element and browsers still render it, so this keeps working and keeps being a problem — drop it, or animate with CSS the reader can turn off through `prefers-reduced-motion`
```

## Good

```js
// @flow
export component Banner() {
  return (
    <p>Sale ends tonight</p>
  );
}
```
