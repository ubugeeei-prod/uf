React Native picks `Button.ios.js` or `Button.android.js` at build time, so each platform ships only its own code. A `Platform.OS` branch ships both and decides at run time.

## Bad

```js path=app/Button.js
// @flow
import { Platform } from "react-native";

export const radius: number = Platform.OS === "ios" ? 8 : 2;
```

```diagnostics
app/Button.js:4:31 prefer platform-specific files for React Native platform branches
```

## Good

```js path=app/Button.ios.js
// @flow
export const radius: number = 8;
```

```js path=app/Button.android.js
// @flow
export const radius: number = 2;
```
