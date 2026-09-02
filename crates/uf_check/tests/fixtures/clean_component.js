// @flow
import * as React from "react";

opaque type UserId = string;

export type Status = "idle" | "loading" | "ready";

type Profile = {| id: UserId, name: string, status: Status |};

export function makeUserId(raw: string): UserId {
  return raw;
}

function first<T>(items: $ReadOnlyArray<T>): T | void {
  return items[0];
}

hook useStatus(initial: Status): Status {
  const [status] = React.useState<Status>(initial);
  return status;
}

component Badge(profile: Profile) {
  const status = useStatus(profile.status);
  return status;
}

export component ProfileList(profiles: $ReadOnlyArray<Profile>) renders* Badge {
  return profiles.map((profile) => <Badge key={profile.id} profile={profile} />);
}

export component Featured(profiles: $ReadOnlyArray<Profile>) renders? Badge {
  const head = first(profiles);
  if (head == null) {
    return null;
  }
  return <Badge profile={head} />;
}
