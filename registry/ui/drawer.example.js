// @flow
import * as React from "@uniflowed/react";

import * as Drawer from "./drawer.js";

/** An order's details, half open and then all the way. */
export component Example() {
  return (
    <Drawer.Root snapPoints={[0.5, 1]}>
      <Drawer.Trigger>Order details</Drawer.Trigger>
      <Drawer.Content handleLabel="Resize the order details">
        <Drawer.Header>
          <Drawer.Title>Order 1024</Drawer.Title>
          <Drawer.Description>Placed this morning, arriving on Thursday.</Drawer.Description>
        </Drawer.Header>
        <p>Two notebooks, a fountain pen and a bottle of ink.</p>
        <Drawer.Footer>
          <Drawer.Close>Close</Drawer.Close>
        </Drawer.Footer>
      </Drawer.Content>
    </Drawer.Root>
  );
}
