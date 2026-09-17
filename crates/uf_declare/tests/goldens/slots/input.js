// @flow

export type Callable = {
  [[call]]: (input: string) => number,
  name: string,
  ...
};
