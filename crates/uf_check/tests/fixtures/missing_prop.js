// @flow
import * as React from "react";

component Greeting(name: string, times: number) {
  return name.repeat(times);
}

export component Page() renders Greeting {
  return <Greeting name="world" />;
}
