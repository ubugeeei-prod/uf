// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Slider } from "./slider.js";

const styles = stylex.create({
  stack: {
    display: "grid",
    gap: ufTokens.space6,
    maxWidth: "20rem",
  },
});

/** A volume, and a price range whose thumbs cannot pass each other. */
export component Example() {
  return (
    <div {...props(styles.stack)}>
      <Slider defaultValue={[50]} labels={["Volume"]} />
      <Slider
        defaultValue={[20, 80]}
        labels={["Minimum price", "Maximum price"]}
        valueText={(value) => `$${value}`}
      />
    </div>
  );
}
