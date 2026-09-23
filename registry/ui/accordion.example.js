// @flow
import * as React from "@uniflowed/react";

import * as Accordion from "./accordion.js";

/** Three questions, one answer open at a time. */
export component Example() {
  return (
    <Accordion.Root defaultValue={["shipping"]}>
      <Accordion.Item value="shipping">
        <Accordion.Trigger>How long does shipping take?</Accordion.Trigger>
        <Accordion.Content>Two working days within the country, five abroad.</Accordion.Content>
      </Accordion.Item>
      <Accordion.Item value="returns">
        <Accordion.Trigger>Can I return an order?</Accordion.Trigger>
        <Accordion.Content>Within thirty days, in the box it came in.</Accordion.Content>
      </Accordion.Item>
      <Accordion.Item disabled value="gifts">
        <Accordion.Trigger>Can I send it as a gift?</Accordion.Trigger>
        <Accordion.Content>Not yet.</Accordion.Content>
      </Accordion.Item>
    </Accordion.Root>
  );
}
