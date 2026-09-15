"use client";
// @flow
import { useState } from "@uniflowed/react";

import type { Sort } from "./table.js";
import {
  Table,
  TableBody,
  TableCaption,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
  TableRowHeader,
  TableRowSelect,
  TableSelectAll,
} from "./table.js";

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
    <Table onSortChange={setSort} sort={sort}>
      <TableCaption>Invoices this month</TableCaption>
      <TableHeader>
        <TableRow>
          <TableHead>
            <TableSelectAll
              checked={all}
              onCheckedChange={(on) => {
                setChosen(new Set(on ? INVOICES.map((invoice) => invoice.id) : []));
              }}
            />
          </TableHead>
          <TableHead column="customer">Customer</TableHead>
          <TableHead column="amount">Amount</TableHead>
          <TableHead>Status</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {sorted(INVOICES, sort).map((invoice) => (
          <TableRow key={invoice.id}>
            <TableCell>
              <TableRowSelect
                checked={chosen.has(invoice.id)}
                label={`Select ${invoice.customer}`}
                onCheckedChange={(on) => {
                  setChosen(toggled(chosen, invoice.id, on));
                }}
              />
            </TableCell>
            <TableRowHeader>{invoice.customer}</TableRowHeader>
            <TableCell>{`$${invoice.amount}`}</TableCell>
            <TableCell>{invoice.status}</TableCell>
          </TableRow>
        ))}
      </TableBody>
    </Table>
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
