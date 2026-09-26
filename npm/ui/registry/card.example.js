// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Button } from "./button.js";
import * as Card from "./card.js";

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
      <Card.Root>
        <Card.Header>
          <Card.Title level={2}>Weekly digest</Card.Title>
          <Card.Description>Sent every Monday at 9:00.</Card.Description>
        </Card.Header>
        <Card.Content>
          <p {...props(styles.body)}>A summary of the week's changes to the projects you follow.</p>
        </Card.Content>
        <Card.Footer>
          <Button tone="primary">Subscribe</Button>
          <Button tone="ghost">Preview</Button>
        </Card.Footer>
      </Card.Root>
    </div>
  );
}
