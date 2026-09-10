// @flow

import * as React from "@uniflowed/react";

import { AuthClient } from "../auth-client.js";
import { SocialFrame } from "../social-frame.js";

export default component SignupPage() renders React.Node {
  return (
    <SocialFrame active="signup">
      <AuthClient mode="signup" />
    </SocialFrame>
  );
}
