"use client";
// @flow
import { Button } from "./button.js";
import { ToastDescription, ToastTitle, Toaster, toast } from "./toast.js";

/** A button that raises a notice, and the corner the notice shows in. */
export component Example() {
  return (
    <>
      <Button
        onClick={() => {
          toast(
            <>
              <ToastTitle>Draft saved</ToastTitle>
              <ToastDescription>Your changes are kept on this device.</ToastDescription>
            </>,
            { duration: null },
          );
        }}
      >
        Save draft
      </Button>
      <Toaster />
    </>
  );
}
