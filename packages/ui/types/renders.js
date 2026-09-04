// @flow
import type * as React from "@uniflowed/react";

export type RendersNode = renders React.Node;
export type RendersElement<Element extends React.Node> = renders Element;

export type HeadlessProps = {
  readonly className?: string,
  readonly children?: React.Node,
  readonly variant?: string,
  readonly size?: string,
  readonly disabled?: boolean,
};

export type HeadlessComponent = component(
  className?: string,
  children?: React.Node,
  variant?: string,
  size?: string,
  disabled?: boolean,
) renders React.Node;
