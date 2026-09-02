// @flow
import * as React from "react";

type NotAComponent = { label: string };

export component Broken() renders NotAComponent {
  return { label: "nope" };
}
