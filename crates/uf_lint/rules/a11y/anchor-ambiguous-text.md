Screen reader users skim a page through its list of links, where each link is heard without the sentence around it. "Click here" or "read more" says nothing in that list (WCAG 2.4.4). Make the link text say where it goes.

## Bad

```js
// @flow
export component Pricing() {
  return (
    <p>
      Plans start at $5. <a href="/pricing">Click here</a>
    </p>
  );
}
```

```diagnostics
app/example.js:5:26 link text "click here" does not say where the link goes, and a screen reader's list of links reads it with nothing around it (WCAG 2.4.4); name the destination instead, such as `<a>read the pricing guide</a>`
```

## Good

```js
// @flow
export component Pricing() {
  return (
    <p>
      Plans start at $5. <a href="/pricing">See every plan</a>
    </p>
  );
}
```
