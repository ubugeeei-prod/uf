// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "./collapsible.js";

const styles = stylex.create({
  list: {
    display: "grid",
    gap: ufTokens.space1,
    margin: 0,
    paddingInlineStart: ufTokens.space6,
  },
});

/** Older replies, tucked away until someone asks for them. */
export component Example() {
  return (
    <Collapsible>
      <CollapsibleTrigger>Show the 3 older replies</CollapsibleTrigger>
      <CollapsibleContent>
        <ul {...props(styles.list)}>
          <li>Looks good to me.</li>
          <li>Could the button say what it saves?</li>
          <li>Changed, thank you.</li>
        </ul>
      </CollapsibleContent>
    </Collapsible>
  );
}
