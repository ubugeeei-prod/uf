// @flow
import * as React from "@uniflowed/react";

import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "./accordion.js";

/** Three questions, one answer open at a time. */
export component Example() {
  return (
    <Accordion defaultValue={["shipping"]}>
      <AccordionItem value="shipping">
        <AccordionTrigger>How long does shipping take?</AccordionTrigger>
        <AccordionContent>Two working days within the country, five abroad.</AccordionContent>
      </AccordionItem>
      <AccordionItem value="returns">
        <AccordionTrigger>Can I return an order?</AccordionTrigger>
        <AccordionContent>Within thirty days, in the box it came in.</AccordionContent>
      </AccordionItem>
      <AccordionItem disabled value="gifts">
        <AccordionTrigger>Can I send it as a gift?</AccordionTrigger>
        <AccordionContent>Not yet.</AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}
