/**
 * @generated SignedSource<<3edca3a6b74cb14b70d5d4593cbd4292>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsAvatar_user$fragmentType } from "./SnsAvatar_user.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsSocialFrame_query$fragmentType: FragmentType;
export type SnsSocialFrame_query$data = {
  readonly viewer: ?{
    readonly handle: string,
    readonly name: string,
    readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  },
  readonly $fragmentType: SnsSocialFrame_query$fragmentType,
};
export type SnsSocialFrame_query$key = {
  readonly $data?: SnsSocialFrame_query$data,
  readonly $fragmentSpreads: SnsSocialFrame_query$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsSocialFrame_query",
  "selections": [
    {
      "alias": null,
      "args": null,
      "concreteType": "User",
      "kind": "LinkedField",
      "name": "viewer",
      "plural": false,
      "selections": [
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "name",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "handle",
          "storageKey": null
        },
        {
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsAvatar_user"
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Query",
  "abstractKey": null
};

(node/*:: as any*/).hash = "d6b0d4d17aa2afee7b9a3f55e07df92c";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsSocialFrame_query$fragmentType,
  SnsSocialFrame_query$data,
>*/);
