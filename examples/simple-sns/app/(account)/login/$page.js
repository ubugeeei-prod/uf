// @flow

import * as React from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";

import { SocialFrame } from "../../_shared/social-frame.js";
import { AuthClient } from "../auth.client.js";

/** Render the sign-in form within the guest navigation shell. */

export component Page() {
  return (
    <SocialFrame active="login" aside={false}>
      <div {...props(styles.authLayout)}>
        <AuthClient mode="login" />
      </div>
    </SocialFrame>
  );
}

const styles = stylex.create({
  authLayout: {
    display       : "flex",
    justifyContent: "center",
    alignItems    : "flex-start",
    minHeight     : { default: "650px", "@media (max-width: 760px)": "0" },
    paddingTop    : { default: "35px", "@media (max-width: 760px)": "0" },
  },
});
