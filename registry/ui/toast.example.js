"use client";
// @flow
import { Button } from "./button.js";
import { toast } from "./toast.js";
import * as Toast from "./toast.js";

/** A button that raises a notice, and the corner the notice shows in. */
export component Example() {
  return (
    <>
      <Button
        onClick={() => {
          toast(
            <>
              <Toast.Title>Draft saved</Toast.Title>
              <Toast.Description>Your changes are kept on this device.</Toast.Description>
            </>,
            { duration: null },
          );
        }}
      >
        Save draft
      </Button>
      <Toast.Region />
    </>
  );
}
