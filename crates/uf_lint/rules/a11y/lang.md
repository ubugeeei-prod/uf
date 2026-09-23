The `lang` on `html` decides the voice and pronunciation a screen reader uses. A value that is not a BCP 47 language tag is ignored, which is the same as having none (WCAG 3.1.1).

## Bad

```js
// @flow
import * as React from "react";

export component Document(children: React.Node) {
  return (
    <html lang="en_GB">
      <body>{children}</body>
    </html>
  );
}
```

```diagnostics
app/example.js:6:11 `lang="en_GB"` is not a language tag, so a screen reader cannot tell what to pronounce this page as and carries on in whatever voice it was already using; write a BCP 47 tag such as `en`, `en-GB` or `ja`
```

## Good

```js
// @flow
import * as React from "react";

export component Document(children: React.Node) {
  return (
    <html lang="en-GB">
      <body>{children}</body>
    </html>
  );
}
```
