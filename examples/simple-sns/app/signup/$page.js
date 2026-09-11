// @flow

import * as React from "@uniflowed/react";
import { SocialFrame } from "../social-frame.js";
import { AuthClient } from "../auth-client.js";

/** Render account creation within the guest navigation shell. */
export component Page() {
  return (
    <SocialFrame active="signup" aside={false}>
      <div className="auth-layout">
        <AuthClient mode="signup" />
      </div>
    </SocialFrame>
  );
}
