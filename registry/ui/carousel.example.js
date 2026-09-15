// @flow
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import {
  Carousel,
  CarouselContent,
  CarouselControls,
  CarouselItem,
  CarouselNext,
  CarouselPrevious,
} from "./carousel.js";

const GUIDES: $ReadOnlyArray<string> = ["Routing", "Styling", "Testing"];

const styles = stylex.create({
  frame: {
    maxWidth: "24rem",
  },
  slide: {
    display: "flex",
    alignItems: "center",
    justifyContent: "center",
    height: "10rem",
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
  },
});

/** Three guides, one at a time, moved between with the buttons under them. */
export component Example() {
  return (
    <Carousel count={GUIDES.length} label="Guides" xstyle={styles.frame}>
      <CarouselContent>
        {GUIDES.map((guide, index) => (
          <CarouselItem index={index} key={guide}>
            <div {...props(styles.slide)}>{guide}</div>
          </CarouselItem>
        ))}
      </CarouselContent>
      <CarouselControls>
        <CarouselPrevious />
        <CarouselNext />
      </CarouselControls>
    </Carousel>
  );
}
