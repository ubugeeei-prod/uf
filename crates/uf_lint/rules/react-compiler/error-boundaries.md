`try`/`catch` around JSX does not catch errors thrown while rendering it: the element is only a description, and React renders it later, outside the `try`. Put an error boundary around the subtree instead.

## Bad

```js
// @flow
import { Chart } from "./chart.js";

export component Panel(data: $ReadOnlyArray<number>) {
  let content;
  try {
    content = <Chart data={data} />;
  } catch {
    content = <p>Could not draw the chart.</p>;
  }
  return content;
}
```

```diagnostics
app/example.js:7:15 Avoid constructing JSX within try/catch. React does not immediately render components when JSX is rendered, so any errors from this component will not be caught by the try/catch. To catch errors in rendering a given component, wrap that component in an error boundary. (https://react.dev/reference/react/Component#catching-rendering-errors-with-an-error-boundary).
```

## Good

```js
// @flow
import { ErrorBoundary } from "@uniflowed/react";
import { Chart } from "./chart.js";

export component Panel(data: $ReadOnlyArray<number>) {
  return (
    <ErrorBoundary fallback={<p>Could not draw the chart.</p>}>
      <Chart data={data} />
    </ErrorBoundary>
  );
}
```
