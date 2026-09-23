Without a `lang` on `html`, a screen reader reads the page in the user's default voice and pronunciation, which garbles text in any other language (WCAG 3.1.1).

## Bad

```js
// @flow
import * as React from "react";

export component Document(children: React.Node) {
  return (
    <html>
      <body>{children}</body>
    </html>
  );
}
```

```diagnostics
app/example.js:6:5 `<html>` names no language, so a screen reader reads the page with the voice and pronunciation of its user's own (WCAG 3.1.1); set `lang` to the language the page is written in, such as `lang="en"`
```

## Good

```js
// @flow
import * as React from "react";

export component Document(children: React.Node) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  );
}
```
