import { describe, expect, it } from "vitest";

import { formatBps, formatCents, parseCentsInput } from "./money";

describe("formatCents", () => {
  it.each([
    [0, "$0.00"],
    [5, "$0.05"],
    [100, "$1.00"],
    [123456, "$1,234.56"],
    [-123456, "-$1,234.56"],
    [-5, "-$0.05"],
    [100000000, "$1,000,000.00"],
    [999, "$9.99"],
  ])("formats %i as %s", (cents, expected) => {
    expect(formatCents(cents)).toBe(expected);
  });

  it("supports explicit plus signs and no symbol", () => {
    expect(formatCents(1500, { sign: "always" })).toBe("+$15.00");
    expect(formatCents(0, { sign: "always" })).toBe("$0.00");
    expect(formatCents(-1500, { sign: "always" })).toBe("-$15.00");
    expect(formatCents(1500, { symbol: false })).toBe("15.00");
    expect(formatCents(-1500, { symbol: false })).toBe("-15.00");
  });

  it("refuses non-integers: the core never sends them, so one is a bug", () => {
    expect(() => formatCents(1.5)).toThrow(RangeError);
    expect(() => formatCents(Number.NaN)).toThrow(RangeError);
    expect(() => formatCents(2 ** 53)).toThrow(RangeError);
  });
});

describe("formatBps", () => {
  it("renders basis points exactly at two decimals", () => {
    expect(formatBps(1999)).toBe("19.99%");
    expect(formatBps(0)).toBe("0.00%");
    expect(formatBps(10000)).toBe("100.00%");
    expect(formatBps(5)).toBe("0.05%");
    expect(formatBps(-250)).toBe("-2.50%");
  });

  it("rounds half away from zero when shortening", () => {
    expect(formatBps(1995, 1)).toBe("20.0%");
    expect(formatBps(1994, 1)).toBe("19.9%");
    expect(formatBps(-1995, 1)).toBe("-20.0%");
    expect(formatBps(1950, 0)).toBe("20%");
    expect(formatBps(1949, 0)).toBe("19%");
    expect(formatBps(50, 0)).toBe("1%");
    expect(formatBps(49, 0)).toBe("0%");
  });
});

describe("parseCentsInput", () => {
  it.each([
    ["1234.56", 123456],
    ["1,234.56", 123456],
    ["$1,234.56", 123456],
    ["-1,234.56", -123456],
    ["(1,234.56)", -123456],
    ["($12)", -1200],
    ["12", 1200],
    ["12.5", 1250],
    [".5", 50],
    ["+7", 700],
    ["7.00-", -700],
    [" 0.07 ", 7],
    ["0", 0],
  ])("parses %s as %i cents", (text, expected) => {
    expect(parseCentsInput(text)).toBe(expected);
  });

  it.each(["", "   ", "$", "abc", "1.234", "1.2.3", "1e5", "12-3", "--5"])("rejects %s", (text) => {
    expect(parseCentsInput(text)).toBeNull();
  });
});
