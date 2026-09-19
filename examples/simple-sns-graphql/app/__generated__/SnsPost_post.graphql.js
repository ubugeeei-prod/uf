/**
 * @generated SignedSource<<9ce93d2c3baa439c1bd0ea184eeb3383>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { SnsAvatar_user$fragmentType } from "./SnsAvatar_user.graphql";
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsPost_post$fragmentType: FragmentType;
export type SnsPost_post$data = {
  readonly author: {
    readonly handle: string,
    readonly name: string,
    readonly $fragmentSpreads: SnsAvatar_user$fragmentType,
  },
  readonly body: string,
  readonly createdAt: string,
  readonly id: string,
  readonly liked: boolean,
  readonly likes: number,
  readonly topic: string,
  readonly $fragmentType: SnsPost_post$fragmentType,
};
export type SnsPost_post$key = {
  readonly $data?: SnsPost_post$data,
  readonly $fragmentSpreads: SnsPost_post$fragmentType,
  ...
};
*/

var node/*: ReaderFragment*/ = {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsPost_post",
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
      "name": "body",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "topic",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "likes",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "liked",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "kind": "ScalarField",
      "name": "createdAt",
      "storageKey": null
    },
    {
      "alias": null,
      "args": null,
      "concreteType": "User",
      "kind": "LinkedField",
      "name": "author",
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
  "type": "Post",
  "abstractKey": null
};

(node/*:: as any*/).hash = "aede9485aadf5865d0ab234145569867";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsPost_post$fragmentType,
  SnsPost_post$data,
>*/);
