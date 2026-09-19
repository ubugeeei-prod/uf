// @flow
export default {
  app: { targets: ["react-native"], rsc: false, router: { root: "app", entry: "index.js" } },
  test: { target: "react-native", runtime: "node" },
};
