/**
 * @generated SignedSource<<90f7c221b2497adce5aef2bb8d3666b7>>
 * @flow
 * @lightSyntaxTransform
 */

/* eslint-disable */

'use strict';

/*::
import type { Fragment, ReaderFragment } from 'relay-runtime';
import type { FragmentType } from "relay-runtime";
declare export opaque type SnsPost_post$fragmentType: FragmentType;
export type SnsPost_post$data = {
  readonly author: {
    readonly avatar: string,
    readonly bio: string,
    readonly handle: string,
    readonly id: string,
    readonly name: string,
    readonly photo: ?string,
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

var node/*: ReaderFragment*/ = (function(){
var v0 = {
  "alias": null,
  "args": null,
  "kind": "ScalarField",
  "name": "id",
  "storageKey": null
};
return {
  "argumentDefinitions": [],
  "kind": "Fragment",
  "metadata": null,
  "name": "SnsPost_post",
  "selections": [
    (v0/*:: as any*/),
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
        (v0/*:: as any*/),
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
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "avatar",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "photo",
          "storageKey": null
        },
        {
          "alias": null,
          "args": null,
          "kind": "ScalarField",
          "name": "bio",
          "storageKey": null
        }
      ],
      "storageKey": null
    }
  ],
  "type": "Post",
  "abstractKey": null
};
})();

(node/*:: as any*/).hash = "8e8f79041fe2516cb9d54d8f0de38555";

export default ((node/*:: as any*/)/*:: as Fragment<
  SnsPost_post$fragmentType,
  SnsPost_post$data,
>*/);
