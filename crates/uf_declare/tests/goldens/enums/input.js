// @flow

export enum Status {
  Active,
  Inactive,
}

export type Holder = { status: Status, ... };
