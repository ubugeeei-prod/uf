// @flow

import * as React from "@uniflowed/react";
import { Suspense, use } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";

import { settingsData } from "../social-queries.js";
import { SocialFrame } from "../social-frame.js";
import { type Settings } from "../social-model.js";
import { SettingsClient } from "./settings-client.js";

type SettingsPageData = {|
  readonly settings: Promise<Settings>,
|};

export function loader(): SettingsPageData {
  return { settings: settingsData() };
}

component SettingsPanel(data: Promise<Settings>) renders React.Node {
  return <SettingsClient initial={use(data)} />;
}

component Skeleton() renders React.Node {
  return <div {...props(styles.skeleton)} />;
}

export default component SettingsPage(data: SettingsPageData) renders React.Node {
  return (
    <SocialFrame active="settings">
      <Suspense fallback={<Skeleton />}>
        <SettingsPanel data={data.settings} />
      </Suspense>
    </SocialFrame>
  );
}

const styles = stylex.create({
  skeleton: {
    backgroundColor: "#ffffff",
    borderColor: "#d9dde5",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    minHeight: 360,
  },
});
