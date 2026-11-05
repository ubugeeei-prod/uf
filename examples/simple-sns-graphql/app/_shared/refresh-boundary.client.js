"use client";
// @flow

import * as React from "@uniflowed/react";

type Props = {
  children: React.Node,
  fallback: React.Node,
};

type Phase = "open" | "failed" | "stuck";

type State = {
  phase     : Phase,
  generation: number,
};

/**
 * Mount the children again after a refetch throws.
 *
 * A network-only refetch that fails is cached and thrown on the next read.
 * That throw belongs to the list, not to the route: the route boundary would
 * replace the page around it. One recovery reads the records already in the
 * store. A second throw in that recovery stays here, on `fallback`.
 *
 * A class, because `getDerivedStateFromError` is React's contract for this
 * and there is no hook that does it.
 */

export class RefreshBoundary extends React.Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { phase: "open", generation: 0 };
  }

  static getDerivedStateFromError(_error: mixed): { phase: Phase } {
    return { phase: "failed" };
  }

  componentDidCatch() {
    this.setState(
      (state) =>
        match (state.generation > 0) {
          true  => { phase: "stuck" },
          false => { phase: "open", generation: 1 },
        },
    );
  }

  render(): React.Node {
    const generation = this.state.generation;

    return match (this.state.phase) {
      "stuck"  => this.props.fallback,
      "failed" => null,
      "open"   => <React.Fragment key={generation}>{this.props.children}</React.Fragment>,
    };
  }
}
