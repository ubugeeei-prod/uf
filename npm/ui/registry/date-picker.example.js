// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import * as DatePicker from "./date-picker.js";

const styles = stylex.create({
  field: {
    display: "grid",
    gap: ufTokens.space1,
    justifyItems: "start",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
  label: {
    fontWeight: ufTokens.weightMedium,
  },
  hint: {
    margin: 0,
    color: ufTokens.muted,
  },
});

/** A start date typed or chosen, with the format it takes said under it. */
export component Example() {
  return (
    <div {...props(styles.field)}>
      <label {...props(styles.label)} htmlFor="start-date">
        Start date
      </label>
      <DatePicker.Root defaultValue="2026-09-14" locale="en-US" today="2026-09-14">
        <DatePicker.Group>
          <DatePicker.Input aria-describedby="start-date-format" id="start-date" />
          <DatePicker.Trigger />
        </DatePicker.Group>
        <DatePicker.Content />
      </DatePicker.Root>
      <p {...props(styles.hint)} id="start-date-format">
        Year, month and day, such as 2026-09-14.
      </p>
    </div>
  );
}
