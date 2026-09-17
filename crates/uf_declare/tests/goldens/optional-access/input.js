// @flow

type Holder = { id: string, ... };

export type Present = Holder["id"];

export type Absent = ?Holder;

export type Optional = Absent?.["id"];
