// @flow
import * as React from "@uniflowed/react";

import {
  Drawer,
  DrawerClose,
  DrawerContent,
  DrawerDescription,
  DrawerFooter,
  DrawerHeader,
  DrawerTitle,
  DrawerTrigger,
} from "./drawer.js";

/** An order's details, half open and then all the way. */
export component Example() {
  return (
    <Drawer snapPoints={[0.5, 1]}>
      <DrawerTrigger>Order details</DrawerTrigger>
      <DrawerContent handleLabel="Resize the order details">
        <DrawerHeader>
          <DrawerTitle>Order 1024</DrawerTitle>
          <DrawerDescription>Placed this morning, arriving on Thursday.</DrawerDescription>
        </DrawerHeader>
        <p>Two notebooks, a fountain pen and a bottle of ink.</p>
        <DrawerFooter>
          <DrawerClose>Close</DrawerClose>
        </DrawerFooter>
      </DrawerContent>
    </Drawer>
  );
}
