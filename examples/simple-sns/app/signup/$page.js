// @flow
import * as React from "@uniflowed/react";
import { SocialFrame } from "../social-frame.js";
import { AuthClient } from "../auth-client.js";
export component Page() {
  return (
    <SocialFrame active="signup" aside={false}>
      <div className="auth-layout">
        <AuthClient mode="signup" />
      </div>
    </SocialFrame>
  );
}
