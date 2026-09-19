/**
 * @generated SignedSource<<f2a56ca873b7f4dd285409a095cf4d4d>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsAvatar_user$fragmentType } from "./../../_shared/__generated__/SnsAvatar_user.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsThread_thread$fragmentType: FragmentType;
export type SnsThread_thread$data = {
  readonly id: string,
  readonly lastMessage: string,
  readonly participant: {
    readonly name: string,
    readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  },
  readonly $fragmentType: SnsThread_thread$fragmentType,
};
export type SnsThread_thread$key = {
  readonly $data?: SnsThread_thread$data,
  readonly $fragmentSpreads: SnsThread_thread$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsThread_thread",
  "selections": [
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "id",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "lastMessage",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "User",
      "kind": "LinkedField",
      "name": "participant",
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
          "args": null,
          "kind": "FragmentSpread",
          "name": "SnsAvatar_user"
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Thread",
  "abstractKey": null
};

(node/*:: as any*/).hash = "3dffbd9d4f14d54bedfd8500dd4b1e15";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsThread_thread$fragmentType,
  SnsThread_thread$data,
>*/);
