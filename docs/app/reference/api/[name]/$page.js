// @flow
//
// One package's API reference: `/reference/api/query` for `@uniflowed/query`.
//
// Prerendered once per package the reference was written for, which is every
// JSON file `tools/docs/api.js` left in `docs/.generated/api/`. A name that is
// not one of them is a 404, like any other page the site does not have.

import * as React from "@uniflowed/react";
import { notFound } from "@uniflowed/router";
import type { Metadata, MetadataArgs } from "@uniflowed/router";

import { ApiReference, packageFor, packages } from "../../../_design/api.js";

export function generateStaticParams(): $ReadOnlyArray<{ readonly name: string }> {
  return packages.map((item) => ({ name: item.slug }));
}

export function generateMetadata(args: MetadataArgs): Metadata {
  const name = typeof args.params.name === "string" ? args.params.name : "";
  const item = packageFor(name);
  return item == null
    ? {}
    : {
        title: `${item.name} · API · uf`,
        description: item.description,
        canonical: `/reference/api/${item.slug}`,
      };
}

export component Page(params: { readonly name: string }) {
  const item = packageFor(params.name);
  if (item == null) {
    notFound();
    return null;
  }
  return <ApiReference item={item} />;
}
