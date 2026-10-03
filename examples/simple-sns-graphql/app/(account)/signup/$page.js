// @flow

import * as React from "@uniflowed/react";

import type { SearchParams } from "@uniflowed/router";

import { props } from "@uniflowed/stylex";

import { SocialFrame } from "../../_shared/social-frame.js";
import { styles as uiStyles } from "../../_shared/ui.js";
import { AccountForm } from "../account.client.js";

export const dynamic = "force-dynamic";

/** No route query: the frame's session preload is the page's only read; the form only mutates. */

export component Page(searchParams: SearchParams) {
  return (
    <SocialFrame active="signup" aside={false}>
      <section>
        <header {...props(uiStyles.pageHeading)}>
          <div>
            <h1 {...props(uiStyles.pageTitle)}>Join Commonplace</h1>
            <p {...props(uiStyles.pageSummary)}>A place to share what you are working on.</p>
          </div>
        </header>
        <AccountForm register={true} />
      </section>
    </SocialFrame>
  );
}
