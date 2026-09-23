// @flow
import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Input } from "./input.js";
import { Label } from "./label.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space2,
    maxWidth: "20rem",
  },
});

/** A labelled search box, an invalid one, and a disabled one. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Label htmlFor="input-example-search">Search projects</Label>
      <Input id="input-example-search" name="q" placeholder="Name or owner" type="search" />
      <Label htmlFor="input-example-slug">Slug</Label>
      <Input aria-invalid="true" defaultValue="My Project" id="input-example-slug" />
      <Label htmlFor="input-example-id">Project id</Label>
      <Input defaultValue="prj_42" disabled id="input-example-id" />
    </div>
  );
}
