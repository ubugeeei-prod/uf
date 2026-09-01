// @flow
import * as React from "@uniflowed/react";
import { Card } from "@uniflowed/ui";

component Page() {
  return (
    <main>
      <Card.Root>
        <Card.Header>
          <Card.Title>uniflowed</Card.Title>
          <Card.Description>Flow at native speed</Card.Description>
        </Card.Header>
        <Card.Body>
          <p>Zero config, native Rust engines, RSC static docs, void deploy target.</p>
        </Card.Body>
      </Card.Root>
    </main>
  );
}

export default Page;
