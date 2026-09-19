/**
 * @generated SignedSource<<277b3cdefb845ffb2e6c9977fb3d128f>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsAvatar_user$fragmentType } from "./SnsAvatar_user.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsComposer_viewer$fragmentType: FragmentType;
export type SnsComposer_viewer$data = {
  readonly name: string,
  readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  readonly $fragmentType: SnsComposer_viewer$fragmentType,
};
export type SnsComposer_viewer$key = {
  readonly $data?: SnsComposer_viewer$data,
  readonly $fragmentSpreads: SnsComposer_viewer$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsComposer_viewer",
  "selections": [
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "name",
      "storageKey": null
    },
    {
      "args": null,
      "kind": "FragmentSpread",
      "name": "SnsAvatar_user"
    }
  ],
  "type": "User",
  "abstractKey": null
};

(node/*:: as any*/).hash = "12f63ee4d4ff33dc7a2cd51e6050aa76";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsComposer_viewer$fragmentType,
  SnsComposer_viewer$data,
>*/);
