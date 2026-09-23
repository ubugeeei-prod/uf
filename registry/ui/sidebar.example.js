// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as Sidebar from "./sidebar.js";

const styles = stylex.create({
  frame: {
    height: "20rem",
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
    overflow: "hidden",
  },
  main: {
    flexGrow: 1,
    padding: ufTokens.space4,
  },
});

/** A dot standing in for an item's icon. */
component Dot() {
  return (
    <svg fill="currentColor" focusable="false" height="8" viewBox="0 0 8 8" width="8">
      <circle cx="4" cy="4" r="4" />
    </svg>
  );
}

/** An app's main navigation beside its page, with a button that collapses it. */
export component Example() {
  return (
    <div {...props(styles.frame)}>
      <Sidebar.Root>
        <Sidebar.Content label="Main">
          <Sidebar.Header>Acme</Sidebar.Header>
          <Sidebar.Item aria-current="page" icon={<Dot />} label="Home">
            Home
          </Sidebar.Item>
          <Sidebar.Item icon={<Dot />} label="Projects">
            Projects
          </Sidebar.Item>
          <Sidebar.Item icon={<Dot />} label="Settings">
            Settings
          </Sidebar.Item>
          <Sidebar.Footer>v1.30.0</Sidebar.Footer>
        </Sidebar.Content>
        <main {...props(styles.main)}>
          <Sidebar.Trigger />
        </main>
      </Sidebar.Root>
    </div>
  );
}
