// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Button } from "./button.js";
import { Card, CardContent, CardDescription, CardFooter, CardHeader, CardTitle } from "./card.js";

const styles = stylex.create({
  frame: {
    maxWidth: "24rem",
  },
  body: {
    margin: 0,
  },
});

/** A card with every part, its title a heading and its actions last. */
export component Example() {
  return (
    <div {...props(styles.frame)}>
      <Card>
        <CardHeader>
          <CardTitle level={2}>Weekly digest</CardTitle>
          <CardDescription>Sent every Monday at 9:00.</CardDescription>
        </CardHeader>
        <CardContent>
          <p {...props(styles.body)}>A summary of the week's changes to the projects you follow.</p>
        </CardContent>
        <CardFooter>
          <Button tone="primary">Subscribe</Button>
          <Button tone="ghost">Preview</Button>
        </CardFooter>
      </Card>
    </div>
  );
}
