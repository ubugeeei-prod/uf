"use client";
// @flow
import { useState } from "@uniflowed/react";

import type { Sort } from "./table.js";
import * as Table from "./table.js";

type Invoice = {|
  readonly id: string,
  readonly customer: string,
  readonly amount: number,
  readonly status: string,
|};

const INVOICES: $ReadOnlyArray<Invoice> = [
  { id: "INV-001", customer: "Ada Lovelace", amount: 250, status: "Paid" },
  { id: "INV-002", customer: "Grace Hopper", amount: 150, status: "Pending" },
  { id: "INV-003", customer: "Alan Turing", amount: 350, status: "Paid" },
];

/** Invoices the page sorts by customer or amount, with rows to choose. */
export component Example() {
  const [sort, setSort] = useState<Sort | null>(null);
  const [chosen, setChosen] = useState<$ReadOnlySet<string>>(new Set());
  const all = chosen.size === INVOICES.length ? true : chosen.size === 0 ? false : "mixed";
  return (
    <Table.Root onSortChange={setSort} sort={sort}>
      <Table.Caption>Invoices this month</Table.Caption>
      <Table.Header>
        <Table.Row>
          <Table.Head>
            <Table.SelectAll
              checked={all}
              onCheckedChange={(on) => {
                setChosen(new Set(on ? INVOICES.map((invoice) => invoice.id) : []));
              }}
            />
          </Table.Head>
          <Table.Head column="customer">Customer</Table.Head>
          <Table.Head column="amount">Amount</Table.Head>
          <Table.Head>Status</Table.Head>
        </Table.Row>
      </Table.Header>
      <Table.Body>
        {sorted(INVOICES, sort).map((invoice) => (
          <Table.Row key={invoice.id}>
            <Table.Cell>
              <Table.RowSelect
                checked={chosen.has(invoice.id)}
                label={`Select ${invoice.customer}`}
                onCheckedChange={(on) => {
                  setChosen(toggled(chosen, invoice.id, on));
                }}
              />
            </Table.Cell>
            <Table.RowHeader>{invoice.customer}</Table.RowHeader>
            <Table.Cell>{`$${invoice.amount}`}</Table.Cell>
            <Table.Cell>{invoice.status}</Table.Cell>
          </Table.Row>
        ))}
      </Table.Body>
    </Table.Root>
  );
}

/** The invoices in the order `sort` asks for. */
function sorted(invoices: $ReadOnlyArray<Invoice>, sort: Sort | null): $ReadOnlyArray<Invoice> {
  if (sort == null) {
    return invoices;
  }
  const way = sort.direction === "ascending" ? 1 : -1;
  const column = sort.column;
  return [...invoices].sort(
    (a, b) =>
      way * (column === "amount" ? a.amount - b.amount : a.customer.localeCompare(b.customer)),
  );
}

/** `chosen` with `id` added or taken away. */
function toggled(chosen: $ReadOnlySet<string>, id: string, on: boolean): $ReadOnlySet<string> {
  const next = new Set(chosen);
  if (on) {
    next.add(id);
  } else {
    next.delete(id);
  }
  return next;
}
