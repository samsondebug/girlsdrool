import { describe, expect, it } from "vitest";

import { compileQuery, parseQuery, tokenize, type Lookups } from "./query-chips";

const lookups: Lookups = {
  accounts: [
    { id: 1, name: "Northbank Checking" },
    { id: 4, name: "Summit Visa" },
  ],
  categories: [
    { id: 10, name: "Fixed", path: "Fixed" },
    { id: 11, name: "Rent", path: "Fixed › Rent" },
    { id: 20, name: "Variable", path: "Variable" },
    { id: 21, name: "Groceries", path: "Variable › Groceries" },
  ],
  ventures: [{ id: 7, name: "Ledgerline" }],
};

describe("tokenize", () => {
  it("splits on whitespace and honours quotes", () => {
    expect(tokenize('cat:"Fixed › Rent" >100 jewel osco')).toEqual([
      "cat:Fixed › Rent",
      ">100",
      "jewel",
      "osco",
    ]);
    expect(tokenize("  ")).toEqual([]);
  });
});

describe("parseQuery", () => {
  it("separates chips from free text", () => {
    const q = parseQuery("account:Summit needs:review >100 amazon");
    expect(q.chips.map((c) => [c.key, c.value])).toEqual([
      ["account", "Summit"],
      ["needs", "review"],
      [">", "100"],
    ]);
    expect(q.text).toEqual(["amazon"]);
  });

  it("treats unknown keys and bare colons as text", () => {
    const q = parseQuery("foo:bar 12:30 trailing:");
    expect(q.chips).toEqual([]);
    expect(q.text).toEqual(["foo:bar", "12:30", "trailing:"]);
  });
});

describe("compileQuery", () => {
  it("resolves names to ids and amounts to cents", () => {
    const { filter, errors } = compileQuery(
      'account:"Northbank Checking" cat:groceries tag:household venture:Ledgerline >100 <=1,000.50 needs:review is:unclassified flag:borrowing status:pending',
      lookups,
    );
    expect(errors).toEqual([]);
    expect(filter.account_ids).toEqual([1]);
    expect(filter.category_ids).toEqual([21]);
    expect(filter.tag_names).toEqual(["household"]);
    expect(filter.venture_ids).toEqual([7]);
    expect(filter.abs_gt_cents).toBe(10000);
    expect(filter.abs_le_cents).toBe(100050);
    expect(filter.needs_review).toBe(true);
    expect(filter.unclassified).toBe(true);
    expect(filter.flags_any).toBe(8);
    expect(filter.status).toBe("pending");
    expect(filter.text).toBeUndefined();
  });

  it("matches categories by path, by name, and by path suffix", () => {
    expect(compileQuery('cat:"Fixed › Rent"', lookups).filter.category_ids).toEqual([11]);
    expect(compileQuery("cat:rent", lookups).filter.category_ids).toEqual([11]);
    expect(compileQuery("cat:11", lookups).filter.category_ids).toEqual([11]);
    expect(compileQuery("cat:Fixed", lookups).filter.category_ids).toEqual([10]);
  });

  it("handles date forms", () => {
    expect(compileQuery("date:2026-08", lookups).filter).toMatchObject({
      date_from: "2026-08-01",
      date_to: "2026-08-31",
    });
    expect(compileQuery("date:2026-02", lookups).filter).toMatchObject({
      date_from: "2026-02-01",
      date_to: "2026-02-28",
    });
    expect(compileQuery("date:2026-07-01..2026-09-30", lookups).filter).toMatchObject({
      date_from: "2026-07-01",
      date_to: "2026-09-30",
    });
    expect(compileQuery("date:2026-09-16", lookups).filter).toMatchObject({
      date_from: "2026-09-16",
      date_to: "2026-09-16",
    });
    expect(compileQuery("since:2026-08-01 until:2026-08-15", lookups).filter).toMatchObject({
      date_from: "2026-08-01",
      date_to: "2026-08-15",
    });
  });

  it("reports unknown names and malformed values instead of guessing", () => {
    const { errors } = compileQuery(
      "account:Nope cat:Nothing venture:Ghost flag:magic status:maybe date:yesterday >abc needs:help",
      lookups,
    );
    expect(errors).toEqual([
      'unknown account "Nope"',
      'unknown category "Nothing"',
      'unknown venture "Ghost"',
      'unknown flag "magic"',
      "status:maybe is not a status",
      "date:yesterday must be YYYY-MM-DD, YYYY-MM or a ..range",
      ">abc is not an amount",
      "needs:help is not a filter (try needs:review)",
    ]);
  });

  it("joins free text for payee and memo matching", () => {
    expect(compileQuery("jewel osco", lookups).filter.text).toBe("jewel osco");
    expect(compileQuery("", lookups).filter.text).toBeUndefined();
  });
});
