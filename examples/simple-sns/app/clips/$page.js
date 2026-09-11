// @flow
import * as React from "@uniflowed/react";
import { SocialFrame } from "../social-frame.js";
import { sessionData } from "../social-queries.js";
import type { Session } from "../social-model.js";
import { Clips } from "./clips-client.js";
import { CLIPS } from "./clip-model.js";
export async function loader(): Promise<{| session: Session |}> {
  return { session: await sessionData() };
}
export component Page(data: {| session: Session |}) {
  return (
    <SocialFrame active="clips" aside={false} session={data.session}>
      <Clips clips={CLIPS} />
    </SocialFrame>
  );
}
