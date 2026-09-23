`accessKey` claims a keyboard shortcut that the browser, the operating system or a screen reader has usually already taken, and it is not announced anywhere a user would find it.

## Bad

```js
// @flow
export component Save() {
  return (
    <button type="submit" accessKey="s">
      Save
    </button>
  );
}
```

```diagnostics
app/example.js:4:27 `accessKey` asks for a keyboard shortcut the browser and the screen reader have already given out, and the modifier that reaches it differs per browser, per platform and per assistive technology; it either does nothing or takes a shortcut away from somebody relying on it, so drop it and give the control a visible, documented way in
```

## Good

```js
// @flow
export component Save() {
  return (
    <button type="submit">Save</button>
  );
}
```
