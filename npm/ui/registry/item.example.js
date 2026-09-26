// @flow
import * as React from "@uniflowed/react";

import { Button } from "./button.js";
import * as Item from "./item.js";

/** A list of team members, each with a decorative avatar and one action. */
export component Example() {
  return (
    <Item.Group aria-label="Team members">
      <Item.Root as="li">
        <Item.Media>
          <Initials>AL</Initials>
        </Item.Media>
        <Item.Content>
          <Item.Title>Ada Lovelace</Item.Title>
          <Item.Description>Owner · ada@example.com</Item.Description>
        </Item.Content>
        <Item.Actions>
          <Button size="sm">Manage</Button>
        </Item.Actions>
      </Item.Root>
      <Item.Root as="li">
        <Item.Media>
          <Initials>GH</Initials>
        </Item.Media>
        <Item.Content>
          <Item.Title>
            <a href="?member=grace">Grace Hopper</a>
          </Item.Title>
          <Item.Description>Member · invited yesterday</Item.Description>
        </Item.Content>
        <Item.Actions>
          <Button size="sm" tone="ghost">
            Resend invite
          </Button>
        </Item.Actions>
      </Item.Root>
    </Item.Group>
  );
}

/** A stand-in avatar the name beside it already says, so nobody hears it. */
component Initials(children: string) {
  return (
    <svg aria-hidden="true" focusable="false" height="32" viewBox="0 0 32 32" width="32">
      <rect fill="currentColor" height="32" opacity="0.15" rx="4" width="32" />
      <text fill="currentColor" fontSize="12" textAnchor="middle" x="16" y="20">
        {children}
      </text>
    </svg>
  );
}
