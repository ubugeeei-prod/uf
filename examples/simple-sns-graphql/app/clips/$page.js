// @flow

import * as React from "@uniflowed/react";

import type { SearchParams } from "@uniflowed/router";

import { SocialFrame } from "../_shared/social-frame.js";
import { CLIPS } from "./clip-model.js";
import { Clips } from "./clips.client.js";

export const dynamic = "force-dynamic";

/** Curated media needs no GraphQL read; the player is the only client island besides the frame's. */

export component Page(searchParams: SearchParams) {
  return (
    <SocialFrame active="clips" aside={false}>
      <Clips clips={CLIPS} />
    </SocialFrame>
  );
}
