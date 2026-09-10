// @flow

import * as React from "@uniflowed/react";

import { AuthClient } from "../auth-client.js";
import { SocialFrame } from "../social-frame.js";

export default component LoginPage() renders React.Node {
  return (
    <SocialFrame active="login">
      <AuthClient mode="login" />
    </SocialFrame>
  );
}
