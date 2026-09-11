// @flow

import * as React from "@uniflowed/react";
import { SocialFrame } from "../social-frame.js";
import { sessionData } from "../social-queries.js";
import type { Session } from "../social-model.js";
import { Clips } from "./clips-client.js";
import { CLIPS } from "./clip-model.js";

/** Resolve only the shared shell identity; curated clip metadata is already local. */
export async function loader(): Promise<{| session: Session |}> {
  return { session: await sessionData() };
}

/** Compose the immersive Clips route inside the shared navigation shell. */
export component Page(data: {| session: Session |}) {
  return (
    <SocialFrame active="clips" aside={false} session={data.session}>
      <Clips clips={CLIPS} />
    </SocialFrame>
  );
}
