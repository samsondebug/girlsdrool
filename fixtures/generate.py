#!/usr/bin/env python3
"""
Kept fixtures (ADR-0028). One explicit, deterministic row list is the single source of truth:
this script writes the seven institutions' CSV exports and EXPECTED.md, showing every running
balance so the known answers can be checked by eye. Nothing here calls the Kept engine.

Run:  python3 fixtures/generate.py      (rewrites fixtures/**/*.csv and fixtures/EXPECTED.md)

Money is integer cents throughout. Dates are civil YYYY-MM-DD. The story covers the three
statement months 2026-07, 2026-08 and 2026-09 for one person with:
  three banks (Northbank checking + savings, Riverside checking), two cards (Summit Visa, Summit
  Amex), one firewalled brokerage (Harbor) and a Venmo balance.
"""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field
from datetime import date, datetime

ROOT = os.path.dirname(os.path.abspath(__file__))


# ---------------------------------------------------------------------------------------------
# Accounts
# ---------------------------------------------------------------------------------------------

@dataclass(frozen=True)
class Account:
    key: str
    name: str
    institution: str
    kind: str
    opening_cents: int
    opening_date: str
    firewalled: bool = False


ACCOUNTS = {
    "nbc": Account("nbc", "Northbank Checking", "Northbank", "checking", 321_455, "2026-07-01"),
    "nbs": Account("nbs", "Northbank Savings", "Northbank", "savings", 1_200_000, "2026-07-01"),
    "rvc": Account("rvc", "Riverside Checking", "Riverside Bank", "checking", 105_000, "2026-07-01"),
    "sv": Account("sv", "Summit Visa", "Summit Card Services", "credit", -182_040, "2026-07-01"),
    "sa": Account("sa", "Summit Amex", "Summit Card Services", "credit", -31_218, "2026-07-01"),
    "hb": Account("hb", "Harbor Brokerage", "Harbor Securities", "brokerage", 40_000, "2026-07-01", True),
    "vm": Account("vm", "Venmo", "Venmo", "payment_app", 0, "2026-07-01"),
}


# ---------------------------------------------------------------------------------------------
# Rows. amount is from the account's point of view (inflow +, outflow −), in cents.
# role tags drive the hand-computed answers: "transfer:<pair>", "income", "refund:<pair>",
# "interest", "fee", "proceeds", "cash", and a category code used from M2 on.
# ---------------------------------------------------------------------------------------------

@dataclass
class Row:
    account: str
    posted: str
    description: str
    amount: int
    category: str                      # category code the M2 rules assign (or 'review')
    effective: str | None = None       # transaction date when the export has one
    status: str = "posted"
    pair: str | None = None            # transfer pair id (both legs share it)
    refund_of: str | None = None       # description of the original purchase (same account)
    kind: str = ""                     # export-specific type column (Sale/Payment/Return/…)
    external_id: str | None = None
    note: str = ""
    extra: dict = field(default_factory=dict)

    @property
    def eff(self) -> str:
        return self.effective or self.posted


R = Row

ROWS: list[Row] = [
    # ---- Northbank Checking -----------------------------------------------------------------
    R("nbc", "2026-07-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-07-02", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-07"),
    R("nbc", "2026-07-03", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent", note="roommate's half of the rent"),
    R("nbc", "2026-07-06", "COMED ELECTRIC", -11_240, "fixed.utilities"),
    R("nbc", "2026-07-10", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-07-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-07-14", "JEWEL-OSCO #3421", -8_734, "variable.groceries"),
    R("nbc", "2026-07-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-07-20", "SUMMIT CARD SERVICES PAYMENT", -182_040, "transfer.card_payment", pair="visa-07"),
    R("nbc", "2026-07-21", "PEOPLES GAS", -4_312, "fixed.utilities"),
    R("nbc", "2026-07-22", "SUMMIT CARD SVCS AMEX PYMT", -31_218, "transfer.card_payment", pair="amex-07"),
    R("nbc", "2026-07-24", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-07-25", "ATM WITHDRAWAL 1120 N STATE", -10_000, "variable.cash"),
    R("nbc", "2026-07-28", "SHELL OIL 57442", -5_218, "variable.fuel"),

    R("nbc", "2026-08-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-08-03", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent"),
    R("nbc", "2026-08-04", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-08"),
    R("nbc", "2026-08-06", "COMED ELECTRIC", -13_875, "fixed.utilities"),
    R("nbc", "2026-08-07", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-08-07", "VENMO CASHOUT", 60_000, "transfer.internal", pair="venmo-08"),
    R("nbc", "2026-08-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-08-13", "JEWEL-OSCO #3421", -11_206, "variable.groceries"),
    R("nbc", "2026-08-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-08-20", "SUMMIT CARD SERVICES PAYMENT", -100_000, "transfer.card_payment", pair="visa-08"),
    R("nbc", "2026-08-21", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-08-21", "PEOPLES GAS", -3_890, "fixed.utilities"),
    R("nbc", "2026-08-22", "SUMMIT CARD SVCS AMEX PYMT", -11_600, "transfer.card_payment", pair="amex-08"),
    R("nbc", "2026-08-26", "SHELL OIL 57442", -4_890, "variable.fuel"),
    R("nbc", "2026-08-28", "ZELLE PAYMENT TO CHRIS PARK", -30_000, "transfer.loan_repayment", note="informal loan repayment 1 of 2"),

    R("nbc", "2026-09-01", "LAKESHORE PROPERTIES RENT", -240_000, "fixed.rent"),
    R("nbc", "2026-09-02", "ZELLE PAYMENT FROM MORGAN AVERY", 120_000, "fixed.rent"),
    R("nbc", "2026-09-03", "ONLINE TRANSFER TO SAV ...5678", -50_000, "transfer.internal", pair="sav-09"),
    R("nbc", "2026-09-04", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-09-08", "COMED ELECTRIC", -12_110, "fixed.utilities"),
    R("nbc", "2026-09-12", "XFINITY", -8_999, "fixed.internet"),
    R("nbc", "2026-09-14", "VENMO *MORGAN AVERY", -8_500, "review", note="bank-funded Venmo payment; purpose never guessed"),
    R("nbc", "2026-09-16", "ACH DEPOSIT HARBOR BROKERAGE", 250_000, "transfer.internal", pair="harbor-09"),
    R("nbc", "2026-09-18", "T-MOBILE", -7_500, "fixed.phone"),
    R("nbc", "2026-09-18", "MERIDIAN CAP ACH PAYROLL", 341_277, "income.salary"),
    R("nbc", "2026-09-19", "JEWEL-OSCO #3421", -9_452, "variable.groceries"),
    R("nbc", "2026-09-20", "SUMMIT CARD SERVICES PAYMENT", -120_000, "transfer.card_payment", pair="visa-09"),
    R("nbc", "2026-09-21", "PEOPLES GAS", -3_655, "fixed.utilities"),
    R("nbc", "2026-09-22", "GEICO AUTO INS ANNUAL", -128_400, "irregular.insurance"),
    R("nbc", "2026-09-22", "SUMMIT CARD SVCS AMEX PYMT", -11_600, "transfer.card_payment", pair="amex-09"),
    R("nbc", "2026-09-28", "ZELLE PAYMENT TO CHRIS PARK", -30_000, "transfer.loan_repayment", note="informal loan repayment 2 of 2"),
    R("nbc", "2026-09-29", "SHELL OIL 57442", -5_033, "variable.fuel"),

    # ---- Northbank Savings ------------------------------------------------------------------
    R("nbs", "2026-07-02", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-07"),
    R("nbs", "2026-07-31", "INTEREST PAYMENT", 1_023, "income.interest"),
    R("nbs", "2026-08-04", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-08"),
    R("nbs", "2026-08-31", "INTEREST PAYMENT", 1_067, "income.interest"),
    R("nbs", "2026-09-03", "ONLINE TRANSFER FROM CHK ...1234", 50_000, "transfer.internal", pair="sav-09"),
    R("nbs", "2026-09-30", "INTEREST PAYMENT", 1_109, "income.interest"),

    # ---- Riverside Checking (transaction date + posted date; debit/credit columns) -----------
    R("rvc", "2026-07-15", "DIRECT DEPOSIT - FREELANCE INVOICE 118", 45_000, "income.other", effective="2026-07-15"),
    R("rvc", "2026-07-23", "COSTCO WHSE #1101", -14_387, "variable.groceries", effective="2026-07-22"),
    R("rvc", "2026-08-12", "DIRECT DEPOSIT - FREELANCE INVOICE 121", 30_000, "income.other", effective="2026-08-11"),
    R("rvc", "2026-08-15", "ATM WITHDRAWAL BANCO AZTECA CDMX", -16_342, "variable.cash", effective="2026-08-14"),
    R("rvc", "2026-08-15", "NON-NETWORK ATM FEE", -300, "debt.fees", effective="2026-08-14"),
    R("rvc", "2026-08-15", "INTL TRANSACTION FEE", -490, "debt.fees", effective="2026-08-14"),
    R("rvc", "2026-08-17", "RESTAURANTE EL CARDENAL", -6_215, "variable.dining", effective="2026-08-16"),
    R("rvc", "2026-09-10", "COSTCO WHSE #1101", -15_790, "variable.groceries", effective="2026-09-09"),
    R("rvc", "2026-09-25", "DIRECT DEPOSIT - FREELANCE INVOICE 125", 60_000, "income.other", effective="2026-09-25"),

    # ---- Summit Visa (card statement convention: Sale positive in the file) -----------------
    R("sv", "2026-07-02", "TRADER JOE S #702", -6_431, "variable.groceries", kind="Sale"),
    R("sv", "2026-07-05", "CHIPOTLE 1192", -1_388, "variable.dining", kind="Sale"),
    R("sv", "2026-07-09", "AMAZON.COM*HG6 AMZN.COM/BILL", -4_299, "variable.shopping", kind="Sale"),
    R("sv", "2026-07-11", "UBER TRIP", -2_314, "variable.transport", kind="Sale"),
    R("sv", "2026-07-15", "WALGREENS #5821", -1_876, "variable.health", kind="Sale"),
    R("sv", "2026-07-19", "PAYMENT - THANK YOU", 182_040, "transfer.card_payment", kind="Payment", pair="visa-07"),
    R("sv", "2026-07-22", "TRADER JOE S #702", -7_215, "variable.groceries", kind="Sale"),
    R("sv", "2026-07-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-07-29", "CHIPOTLE 1192", -1_527, "variable.dining", kind="Sale"),
    R("sv", "2026-07-31", "INTEREST CHARGE ON PURCHASES", -2_287, "debt.interest", kind="Interest"),

    R("sv", "2026-08-03", "TRADER JOE S #702", -5_890, "variable.groceries", kind="Sale"),
    R("sv", "2026-08-09", "TARGET 00012345", -8_417, "variable.shopping", kind="Sale"),
    R("sv", "2026-08-12", "UBER TRIP", -1_890, "variable.transport", kind="Sale"),
    R("sv", "2026-08-16", "TRADER JOE S #702", -6_702, "variable.groceries", kind="Sale"),
    R("sv", "2026-08-19", "PAYMENT - THANK YOU", 100_000, "transfer.card_payment", kind="Payment", pair="visa-08"),
    R("sv", "2026-08-20", "TARGET 00012345", 8_417, "variable.shopping", kind="Return", refund_of="TARGET 00012345"),
    R("sv", "2026-08-24", "CHIPOTLE 1192", -1_412, "variable.dining", kind="Sale"),
    R("sv", "2026-08-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-08-31", "INTEREST CHARGE ON PURCHASES", -1_944, "debt.interest", kind="Interest"),
    # the pending row appears in the August export only; see PENDING_AMAZON below

    R("sv", "2026-09-01", "AMAZON.COM*2K4 AMZN.COM/BILL", -6_250, "variable.shopping", kind="Sale", effective="2026-08-30"),
    R("sv", "2026-09-06", "TRADER JOE S #702", -7_340, "variable.groceries", kind="Sale"),
    R("sv", "2026-09-10", "WALGREENS #5821", -2_210, "variable.health", kind="Sale"),
    R("sv", "2026-09-13", "UBER TRIP", -2_760, "variable.transport", kind="Sale"),
    R("sv", "2026-09-19", "PAYMENT - THANK YOU", 120_000, "transfer.card_payment", kind="Payment", pair="visa-09"),
    R("sv", "2026-09-21", "TRADER JOE S #702", -6_118, "variable.groceries", kind="Sale"),
    R("sv", "2026-09-26", "NETFLIX.COM", -1_549, "fixed.subscriptions", kind="Sale"),
    R("sv", "2026-09-27", "CHIPOTLE 1192", -1_495, "variable.dining", kind="Sale"),
    R("sv", "2026-09-30", "INTEREST CHARGE ON PURCHASES", -1_872, "debt.interest", kind="Interest"),

    # ---- Summit Amex ------------------------------------------------------------------------
    R("sa", "2026-07-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-07-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-07-22", "PAYMENT - THANK YOU", 31_218, "transfer.card_payment", kind="Payment", pair="amex-07"),
    R("sa", "2026-08-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-08-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-08-22", "PAYMENT - THANK YOU", 11_600, "transfer.card_payment", kind="Payment", pair="amex-08"),
    R("sa", "2026-09-15", "LINEAR.APP", -9_600, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-09-15", "VERCEL INC", -2_000, "venture.operating_expense", kind="Sale"),
    R("sa", "2026-09-22", "PAYMENT - THANK YOU", 11_600, "transfer.card_payment", kind="Payment", pair="amex-09"),

    # ---- Harbor Brokerage (firewalled; cash balance only) -----------------------------------
    R("hb", "2026-07-15", "DIVIDEND", 1_842, "income.interest", kind="Dividend", extra={"symbol": "VTI", "qty": "", "price": ""}),
    R("hb", "2026-09-12", "SELL", 250_000, "transfer.securities_sale_proceeds", kind="Sell", extra={"symbol": "VTI", "qty": "20", "price": "125.00"}),
    R("hb", "2026-09-15", "ACH TRANSFER TO NORTHBANK ...1234", -250_000, "transfer.internal", kind="Transfer Out", pair="harbor-09", extra={"symbol": "", "qty": "", "price": ""}),

    # ---- Venmo (balance-held rows only; bank-funded rows are skipped by the profile) --------
    R("vm", "2026-08-05", "Chris Park", 60_000, "review", kind="Payment", external_id="4211000001", note="til payday",
      extra={"time": "14:03:11", "from": "Chris Park", "to": "Dave Kept", "funding": "Venmo balance", "destination": ""}),
    R("vm", "2026-08-06", "Northbank Bank *1234", -60_000, "transfer.internal", kind="Standard Transfer", external_id="4211000002", pair="venmo-08",
      extra={"time": "09:15:40", "from": "Dave Kept", "to": "", "funding": "Venmo balance", "destination": "Northbank Bank *1234"}),
    R("vm", "2026-09-20", "Morgan Avery", 4_200, "review", kind="Payment", external_id="4211000004", note="dinner",
      extra={"time": "20:11:37", "from": "Morgan Avery", "to": "Dave Kept", "funding": "Venmo balance", "destination": ""}),
]

# The Visa August export carries this pending row; the September export carries it posted
# (ROWS above, 2026-09-01) with a longer descriptor. Dedup must update, not duplicate.
PENDING_AMAZON = R("sv", "2026-08-30", "AMAZON.COM", -6_250, "variable.shopping", kind="Sale", status="pending")

# This Venmo payment was funded straight from the bank, so it never touched the Venmo balance.
# The profile skips it (reported, not silent); the bank's own row (NBC 2026-09-14) is the ledger.
VENMO_BANK_FUNDED = R("vm", "2026-09-13", "Morgan Avery", -8_500, "review", kind="Payment", external_id="4211000003", note="concert tix",
                      extra={"time": "19:42:05", "from": "Dave Kept", "to": "Morgan Avery", "funding": "Northbank Checking", "destination": ""})

# The Northbank Aug+Sep overlap export repeats every row and writes one descriptor differently.
OVERLAP_VARIANT = ("2026-08-13", "JEWEL-OSCO #3421", "JEWEL-OSCO #3421 CHICAGO")

# ---------------------------------------------------------------------------------------------
# M2: the ordered rule set (payee_norm contains → category, optional venture) and the heuristics
# that run after it. Written down here, emitted to rules.json for the tests and to EXPECTED.md.
# ---------------------------------------------------------------------------------------------

VENTURE = {"name": "Ledgerline", "status": "fund", "cash_cap_cents": 500_000}

RULES: list[tuple[str, str, str, str | None]] = [
    # (name, payee_norm contains, category code, venture)
    ("Rent", "lakeshore properties", "fixed.rent", None),
    ("Rent share from Morgan", "zelle payment from morgan avery", "fixed.rent", None),
    ("ComEd", "comed", "fixed.utilities", None),
    ("Peoples Gas", "peoples gas", "fixed.utilities", None),
    ("Payroll", "meridian cap", "income.salary", None),
    ("Xfinity", "xfinity", "fixed.internet", None),
    ("T-Mobile", "t mobile", "fixed.phone", None),
    ("Jewel-Osco", "jewel osco", "variable.groceries", None),
    ("Trader Joe's", "trader joe", "variable.groceries", None),
    ("Costco", "costco", "variable.groceries", None),
    ("Shell", "shell oil", "variable.fuel", None),
    ("Chipotle", "chipotle", "variable.dining", None),
    ("Restaurants abroad", "restaurante", "variable.dining", None),
    ("Amazon", "amazon", "variable.shopping", None),
    ("Target", "target", "variable.shopping", None),
    ("Uber", "uber", "variable.transport", None),
    ("Walgreens", "walgreens", "variable.health", None),
    ("Netflix", "netflix", "fixed.subscriptions", None),
    ("GEICO annual", "geico", "irregular.insurance", None),
    ("Freelance", "freelance invoice", "income.other", None),
    ("Savings interest", "interest payment", "income.interest", None),
    ("Dividends", "dividend", "income.interest", None),
    ("Linear (Ledgerline)", "linear app", "venture.operating_expense", "Ledgerline"),
    ("Vercel (Ledgerline)", "vercel", "venture.operating_expense", "Ledgerline"),
    ("Repayment to Chris", "zelle payment to chris park", "transfer.loan_repayment", None),
]

# M3: the mutated August export transposes two digits of one amount (−112.06 → −121.06), so the
# ledger built from it is 9.00 short of the bank's statement closing.
MUTATION = ("2026-08-13", "JEWEL-OSCO #3421", -12_106)

PAYMENT_APP_WORDS = ("venmo", "zelle", "cash app", "paypal")


def rule_for(norm: str) -> tuple[str, str, str, str | None] | None:
    for rule in RULES:
        if rule[1] in norm:
            return rule
    return None


def automation(row: Row) -> tuple[str | None, str, set[str]]:
    """What rules → heuristics → linking leave on a row: (category code, why, flags)."""
    norm = normalize(row.description)
    if row.pair:
        kind = "card_payment" if ACCOUNTS[[l.account for l in pair_legs(row.pair) if l.amount > 0][0]].kind == "credit" else "internal"
        flags = set()
        if ACCOUNTS[row.account].firewalled and row.amount < 0:
            flags.add("needs_review")  # firewall touch awaiting acknowledgment
        return f"transfer.{kind}", f"heuristic:{kind}_pair", flags
    rule = rule_for(norm)
    if rule:
        return rule[2], f"rule:{rule[0]}", set()
    if row.kind == "Sell":
        return "transfer.securities_sale_proceeds", "heuristic:securities_sale", {"securities_sale"}
    if "fee" in norm:
        return "debt.fees", "heuristic:fee_charge", {"fee"}
    if "interest" in norm and row.amount < 0:
        return "debt.interest", "heuristic:interest_charge", {"interest"}
    if "interest" in norm and row.amount > 0:
        return "income.interest", "heuristic:interest_income", {"interest"}
    if "atm" in norm:
        return None, "heuristic:atm_withdrawal", {"cash_withdrawal", "needs_review"}
    if row.account == "vm" or any(w in norm for w in PAYMENT_APP_WORDS):
        return None, "heuristic:payment_app_row", {"payment_app_unknown", "needs_review"}
    return None, "unclassified", {"needs_review"}


def pair_legs(pair_id: str) -> list[Row]:
    return [r for r in ROWS if r.pair == pair_id]


# ---------------------------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------------------------

def money(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    a = abs(cents)
    return f"{sign}{a // 100:,}.{a % 100:02d}"


def plain(cents: int) -> str:
    sign = "-" if cents < 0 else ""
    a = abs(cents)
    return f"{sign}{a // 100}.{a % 100:02d}"


def mdy(iso: str) -> str:
    d = date.fromisoformat(iso)
    return f"{d.month:02d}/{d.day:02d}/{d.year}"


def month_end(month: str) -> str:
    import calendar
    y, m = int(month[:4]), int(month[5:7])
    return f"{month}-{calendar.monthrange(y, m)[1]:02d}"


def month_of(iso: str) -> str:
    return iso[:7]


def rows_for(account: str, month: str | None = None) -> list[Row]:
    out = [r for r in ROWS if r.account == account and (month is None or month_of(r.posted) == month)]
    return sorted(out, key=lambda r: (r.posted, ROWS.index(r)))


MONTHS = ["2026-07", "2026-08", "2026-09"]


def closing(account: str, through_month: str) -> int:
    bal = ACCOUNTS[account].opening_cents
    for r in rows_for(account):
        if month_of(r.posted) <= through_month:
            bal += r.amount
    return bal


def write(path: str, text: str) -> None:
    full = os.path.join(ROOT, path)
    os.makedirs(os.path.dirname(full), exist_ok=True)
    with open(full, "w", newline="\n") as f:
        f.write(text)


# ---------------------------------------------------------------------------------------------
# Payee normalisation and Jaro–Winkler, re-implemented here independently of the engine so the
# expected similarities are hand answers, not engine output (ADR-0018 / ADR-0037).
# ---------------------------------------------------------------------------------------------

PREFIXES = ["pos debit ", "checkcard ", "debit card purchase ", "sq *", "tst* ", "tst*", "paypal *", "pp*"]


def normalize(payee: str) -> str:
    s = payee.lower().strip()
    for p in PREFIXES:
        if s.startswith(p):
            s = s[len(p):]
    if "*" in s and len(s.split("*", 1)[0].strip()) >= 3:
        s = s.split("*", 1)[0]
    s = "".join(ch if ch.isalnum() else " " for ch in s)
    tokens = [t for t in s.split() if not t.isdigit() and not (len(t) <= 5 and any(c.isdigit() for c in t))]
    return " ".join(tokens)


def jaro_winkler(a: str, b: str) -> float:
    if a == b:
        return 1.0
    if not a or not b:
        return 0.0
    window = max(len(a), len(b)) // 2 - 1
    a_flags = [False] * len(a)
    b_flags = [False] * len(b)
    matches = 0
    for i, ca in enumerate(a):
        lo, hi = max(0, i - window), min(len(b), i + window + 1)
        for j in range(lo, hi):
            if not b_flags[j] and b[j] == ca:
                a_flags[i] = b_flags[j] = True
                matches += 1
                break
    if matches == 0:
        return 0.0
    a_m = [a[i] for i in range(len(a)) if a_flags[i]]
    b_m = [b[j] for j in range(len(b)) if b_flags[j]]
    transpositions = sum(1 for x, y in zip(a_m, b_m) if x != y) // 2
    jaro = (matches / len(a) + matches / len(b) + (matches - transpositions) / matches) / 3
    prefix = 0
    for x, y in zip(a, b):
        if x == y and prefix < 4:
            prefix += 1
        else:
            break
    return jaro + prefix * 0.1 * (1 - jaro)


# ---------------------------------------------------------------------------------------------
# CSV writers, one per institution format
# ---------------------------------------------------------------------------------------------

def csv_field(s: str) -> str:
    if any(c in s for c in ",\"\n"):
        return '"' + s.replace('"', '""') + '"'
    return s


def northbank(account: str, rows: list[Row], opening: int, variant: tuple | None = None,
              mutate: tuple | None = None) -> str:
    lines = ["Date,Description,Amount,Running Bal."]
    bal = opening
    for r in rows:
        amount = r.amount
        if mutate and (r.posted, r.description) == (mutate[0], mutate[1]):
            amount = mutate[2]
        bal += amount
        desc = r.description
        if variant and (r.posted, r.description) == (variant[0], variant[1]):
            desc = variant[2]
        lines.append(",".join([mdy(r.posted), csv_field(desc), csv_field(money(amount)), csv_field(money(bal))]))
    return "\n".join(lines) + "\n"


def riverside(rows: list[Row], opening: int) -> str:
    lines = ["Transaction Date,Posted Date,Description,Debit,Credit,Balance,Currency"]
    bal = opening
    for r in rows:
        bal += r.amount
        debit = plain(-r.amount) if r.amount < 0 else ""
        credit = plain(r.amount) if r.amount > 0 else ""
        lines.append(",".join([r.eff, r.posted, csv_field(r.description), debit, credit, plain(bal), "USD"]))
    return "\n".join(lines) + "\n"


def summit(rows: list[Row]) -> str:
    # card statement convention: Sale/Interest/Fee positive, Payment/Return negative
    lines = ["Transaction Date,Post Date,Description,Type,Amount,Status"]
    for r in rows:
        file_amount = -r.amount
        status = "Pending" if r.status == "pending" else "Posted"
        lines.append(",".join([mdy(r.eff), mdy(r.posted), csv_field(r.description), r.kind, plain(file_amount), status]))
    return "\n".join(lines) + "\n"


def harbor(rows: list[Row], opening: int) -> str:
    lines = ["Date,Activity,Symbol,Quantity,Price,Amount,Cash Balance"]
    bal = opening
    for r in rows:
        bal += r.amount
        x = r.extra
        lines.append(",".join([mdy(r.posted), csv_field(r.description), x["symbol"], x["qty"], x["price"], plain(r.amount), plain(bal)]))
    return "\n".join(lines) + "\n"


def venmo(rows: list[Row]) -> str:
    # Venmo's export starts with two preamble lines before the header; amounts read "+ $600.00".
    lines = [
        "Account Statement - (@dave-kept) - Jul 1 2026 to Sep 30 2026",
        "Account Activity",
        "ID,Datetime,Type,Status,Note,From,To,Amount (total),Funding Source,Destination",
    ]
    for r in rows:
        x = r.extra
        sign = "+" if r.amount > 0 else "-"
        amount = f"{sign} ${abs(r.amount) // 100:,}.{abs(r.amount) % 100:02d}"
        lines.append(",".join([
            r.external_id or "", f"{r.posted}T{x['time']}", r.kind, "Complete", csv_field(r.note),
            csv_field(x["from"]), csv_field(x["to"]), csv_field(amount), csv_field(x["funding"]), csv_field(x["destination"]),
        ]))
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------------------------------------
# Emit files
# ---------------------------------------------------------------------------------------------

def emit_csvs() -> dict[str, str]:
    files: dict[str, str] = {}

    for m in MONTHS:
        prev = closing("nbc", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["nbc"].opening_cents
        files[f"northbank/northbank_checking_{m}.csv"] = northbank("nbc", rows_for("nbc", m), prev)
        prev_s = closing("nbs", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["nbs"].opening_cents
        files[f"northbank/northbank_savings_{m}.csv"] = northbank("nbs", rows_for("nbs", m), prev_s)
    # byte-identical duplicate of July
    files["northbank/northbank_checking_2026-07_copy.csv"] = files["northbank/northbank_checking_2026-07.csv"]
    # overlapping Aug+Sep export with one descriptor variant
    files["northbank/northbank_checking_2026-08_09_overlap.csv"] = northbank(
        "nbc", rows_for("nbc", "2026-08") + rows_for("nbc", "2026-09"), closing("nbc", "2026-07"), OVERLAP_VARIANT)
    # August with one amount mutated (M3): never imported alongside the real August file
    files["northbank/northbank_checking_2026-08_mutated.csv"] = northbank(
        "nbc", rows_for("nbc", "2026-08"), closing("nbc", "2026-07"), mutate=MUTATION)

    for m in MONTHS:
        prev = closing("rvc", f"2026-{int(m[-2:]) - 1:02d}") if m != "2026-07" else ACCOUNTS["rvc"].opening_cents
        files[f"riverside/riverside_checking_{m}.csv"] = riverside(rows_for("rvc", m), prev)
    files["riverside/riverside_checking_2026-07_eur.csv"] = files["riverside/riverside_checking_2026-07.csv"].replace(",USD", ",EUR")

    for m in MONTHS:
        rows = rows_for("sv", m)
        if m == "2026-08":
            rows = rows + [PENDING_AMAZON]
        files[f"summit/summit_visa_{m}.csv"] = summit(rows)
        files[f"summit/summit_amex_{m}.csv"] = summit(rows_for("sa", m))

    files["harbor/harbor_brokerage_2026-Q3.csv"] = harbor(rows_for("hb"), ACCOUNTS["hb"].opening_cents)

    vm_rows = sorted(rows_for("vm") + [VENMO_BANK_FUNDED], key=lambda r: r.posted)
    files["venmo/venmo_2026-Q3.csv"] = venmo(vm_rows)

    for path, text in files.items():
        write(path, text)
    return files


# ---------------------------------------------------------------------------------------------
# EXPECTED.md
# ---------------------------------------------------------------------------------------------

def expected_md(files: dict[str, str]) -> str:
    out: list[str] = []
    p = out.append
    p("# Fixture known answers")
    p("")
    p("Generated by `fixtures/generate.py` from one explicit row list; every figure below is derived from")
    p("that list with plain arithmetic (running balances are shown so they can be checked by eye). The")
    p("engine is wrong until the fixture is shown wrong, and that showing is an ADR (ADR-0028).")
    p("")
    p("Period: 2026-07-01 through 2026-09-30. Money in dollars with cents; the engine holds integer cents.")
    p("")

    # Accounts
    p("## Accounts")
    p("")
    p("| key | name | institution | kind | opening date | opening balance | firewalled |")
    p("|---|---|---|---|---|---|---|")
    for a in ACCOUNTS.values():
        p(f"| {a.key} | {a.name} | {a.institution} | {a.kind} | {a.opening_date} | {money(a.opening_cents)} | {'yes' if a.firewalled else 'no'} |")
    p("")

    # Files
    p("## Files")
    p("")
    p("| file | account | profile | rows | note |")
    p("|---|---|---|---|---|")
    notes = {
        "northbank/northbank_checking_2026-07_copy.csv": "byte-identical to the July file: file-level no-op",
        "northbank/northbank_checking_2026-08_09_overlap.csv": "Aug+Sep rows again; one descriptor variant → quarantine",
        "northbank/northbank_checking_2026-08_mutated.csv": "August with one amount transposed (−112.06 → −121.06): the M3 off scenario, imported instead of the real August file",
        "riverside/riverside_checking_2026-07_eur.csv": "Currency column reads EUR: whole batch rejected (Unsupported)",
        "summit/summit_visa_2026-08.csv": "carries the pending AMAZON.COM row",
        "summit/summit_visa_2026-09.csv": "carries the same Amazon row posted on 2026-09-01",
        "venmo/venmo_2026-Q3.csv": "two preamble lines; one bank-funded row skipped by the profile",
    }
    profile_of = {"northbank": "northbank_csv", "riverside": "riverside_csv", "summit": "summit_card_csv", "harbor": "harbor_brokerage_csv", "venmo": "venmo_csv"}
    acct_of_file = {}
    for path in sorted(files):
        inst = path.split("/")[0]
        base = os.path.basename(path)
        key = {"northbank_checking": "nbc", "northbank_savings": "nbs", "riverside_checking": "rvc", "summit_visa": "sv",
               "summit_amex": "sa", "harbor_brokerage": "hb", "venmo": "vm"}[base.rsplit("_20", 1)[0]]
        acct_of_file[path] = key
        data_rows = len([ln for ln in files[path].strip().split("\n")]) - (3 if inst == "venmo" else 1)
        p(f"| `{path}` | {key} | {profile_of[inst]} | {data_rows} | {notes.get(path, '')} |")
    p("")

    # Per-account ledgers with running balances
    p("## Ledger per account (posted rows, running balance)")
    p("")
    for a in ACCOUNTS.values():
        p(f"### {a.name} (`{a.key}`)")
        p("")
        p(f"Opening {a.opening_date}: **{money(a.opening_cents)}**")
        p("")
        p("| posted | effective | description | amount | running | role |")
        p("|---|---|---|---:|---:|---|")
        bal = a.opening_cents
        for r in rows_for(a.key):
            bal += r.amount
            role = r.category
            if r.pair:
                role += f" · pair `{r.pair}`"
            if r.refund_of:
                role += f" · refund of `{r.refund_of}`"
            p(f"| {r.posted} | {r.eff} | {r.description} | {money(r.amount)} | {money(bal)} | {role} |")
        p("")
        p("Closing per statement month:")
        p("")
        for m in MONTHS:
            p(f"- {m}: **{money(closing(a.key, m))}**")
        p("")

    # Dedup
    p("## Dedup outcomes (M1)")
    p("")
    p("Import order: every monthly/quarterly file once, then the duplicate, overlap and EUR files.")
    p("")
    total_rows = len(ROWS)
    p(f"- Rows in the ledger after importing each file once: **{total_rows}** (the Visa pending row counts once: it is")
    p("  inserted pending from the August file and updated to posted by the September file).")
    p("- `northbank_checking_2026-07_copy.csv`: same account, profile and SHA-256 → batch recorded with inserted 0,")
    p(f"  skipped {len(rows_for('nbc', '2026-07'))}, reason `duplicate_file_of_batch`.")
    aug_sep = len(rows_for("nbc", "2026-08")) + len(rows_for("nbc", "2026-09"))
    p(f"- `northbank_checking_2026-08_09_overlap.csv`: {aug_sep} rows; {aug_sep - 1} skipped by `source_row_hash`;")
    v = OVERLAP_VARIANT
    sim = jaro_winkler(normalize(v[1]), normalize(v[2]))
    p(f"  1 quarantined: `{v[2]}` {v[0]} vs ledger `{v[1]}` — same account, same amount, same date,")
    p(f"  payee_norm `{normalize(v[2])}` vs `{normalize(v[1])}`, Jaro–Winkler {sim:.4f} ≥ 0.85, both posted → suspected duplicate, not inserted.")
    pend_sim = jaro_winkler(normalize(PENDING_AMAZON.description), normalize("AMAZON.COM*2K4 AMZN.COM/BILL"))
    p(f"- Visa pending → posted: August row `{PENDING_AMAZON.description}` pending {PENDING_AMAZON.posted} and September row")
    p(f"  `AMAZON.COM*2K4 AMZN.COM/BILL` posted 2026-09-01: same amount, 2 days apart, payee_norm `{normalize(PENDING_AMAZON.description)}`")
    p(f"  vs `{normalize('AMAZON.COM*2K4 AMZN.COM/BILL')}`, Jaro–Winkler {pend_sim:.4f} → the pending row is updated to posted 2026-09-01 with the")
    p("  new descriptor; the September batch reports updated 1, inserted 8.")
    p(f"- `venmo_2026-Q3.csv`: 4 data rows; 1 skipped by the profile's funding-source rule ({VENMO_BANK_FUNDED.external_id},")
    p(f"  funded from `{VENMO_BANK_FUNDED.extra['funding']}`), 3 inserted; every inserted row flagged `payment_app_unknown | needs_review`.")
    p("- `riverside_checking_2026-07_eur.csv`: rejected before any write with `Unsupported` (currency EUR).")
    p("- Re-importing every file a second time: 0 inserted, 0 updated, 0 user fields changed (`import_idempotent`).")
    p("")

    # Normalisation table
    p("## payee_norm for descriptors the tests check (ADR-0018)")
    p("")
    p("| descriptor | payee_norm |")
    p("|---|---|")
    for d in ["AMAZON.COM*2K4 AMZN.COM/BILL", "AMAZON.COM", "JEWEL-OSCO #3421", "JEWEL-OSCO #3421 CHICAGO", "TRADER JOE S #702",
              "SHELL OIL 57442", "ONLINE TRANSFER TO SAV ...5678", "PAYMENT - THANK YOU", "WALGREENS #5821", "ZELLE PAYMENT FROM MORGAN AVERY"]:
        p(f"| `{d}` | `{normalize(d)}` |")
    p("")

    # Transfer pairs (M2)
    p("## Transfer pairs (M2)")
    p("")
    p("| pair | out | in | kind |")
    p("|---|---|---|---|")
    pairs: dict[str, list[Row]] = {}
    for r in ROWS:
        if r.pair:
            pairs.setdefault(r.pair, []).append(r)
    for pid, legs in pairs.items():
        out_leg = next(l for l in legs if l.amount < 0)
        in_leg = next(l for l in legs if l.amount > 0)
        kind = "card_payment" if ACCOUNTS[in_leg.account].kind == "credit" else "internal"
        p(f"| `{pid}` | {out_leg.account} {out_leg.posted} {money(out_leg.amount)} | {in_leg.account} {in_leg.posted} {money(in_leg.amount)} | {kind} |")
    p("")
    p("Refund link (M2): Visa `TARGET 00012345` 2026-08-20 +84.17 ↔ Visa `TARGET 00012345` 2026-08-09 −84.17 (exact payee and amount, 11 days).")
    p("")

    # Spending vs cash (M2)
    p("## Spending view vs cash view, 2026-07-01..2026-09-30 (M2)")
    p("")
    def auto_cat(r: Row) -> str | None:
        return automation(r)[0]
    # ARCHITECTURE §5.2: a row whose category root is income or transfer is not spending, whether
    # or not it is one leg of a linked pair (the loan repayments to Chris are transfer-root rows).
    spend_rows = [r for r in ROWS if r.pair is None
                  and not (auto_cat(r) or "").startswith(("income.", "transfer."))]
    spending_out = -sum(r.amount for r in spend_rows if r.amount < 0)
    refunds = sum(r.amount for r in spend_rows if r.amount > 0 and r.refund_of)
    reimburse = sum(r.amount for r in spend_rows if r.amount > 0 and not r.refund_of and auto_cat(r) is not None)
    review_in = sum(r.amount for r in spend_rows if r.amount > 0 and auto_cat(r) is None)
    p("Spending view = every non-transfer, non-income, non-proceeds leaf row, by category; linked refunds and")
    p("same-category reimbursements net against their category; rows still in review are listed separately.")
    p("")
    p(f"- Gross outflows: **{money(spending_out)}**")
    p(f"- Linked refunds: {money(refunds)} (Target return)")
    p(f"- Same-category reimbursements: {money(reimburse)} (roommate rent split, 3 × 1,200.00)")
    p(f"- Net spending: **{money(spending_out - refunds - reimburse)}**")
    p(f"- Positive rows awaiting review (not spending, not income until classified): {money(review_in)}")
    p("")
    cash_accounts = {k for k, a in ACCOUNTS.items() if a.kind in ("checking", "savings", "cash", "payment_app")}
    cash_rows = [r for r in ROWS if r.account in cash_accounts]
    pair_accounts = {}
    for pid, legs in pairs.items():
        pair_accounts[pid] = {l.account for l in legs}
    def crosses_out_of_cash(r: Row) -> bool:
        return r.pair is not None and not pair_accounts[r.pair] <= cash_accounts
    cash_out = -sum(r.amount for r in cash_rows if r.amount < 0 and (r.pair is None or crosses_out_of_cash(r)))
    cash_in = sum(r.amount for r in cash_rows if r.amount > 0 and (r.pair is None or crosses_out_of_cash(r)))
    p("Cash view = rows on cash accounts (nbc, nbs, rvc, vm) by date; transfers between two cash accounts net")
    p("to zero and are excluded; card payments and the brokerage deposit count on their own dates.")
    p("")
    p(f"- Cash outflows: **{money(cash_out)}**")
    p(f"- Cash inflows: **{money(cash_in)}**")
    net_cash = sum(closing(k, '2026-09') - ACCOUNTS[k].opening_cents for k in cash_accounts)
    p(f"- Net change in cash accounts (Σ closing − opening): **{money(net_cash)}** = inflows − outflows ({money(cash_in - cash_out)})")
    p(f"- Spending-view gross outflows minus cash-view outflows: **{money(spending_out - cash_out)}**")
    p("  (card purchases counted on the cards vs card payments counted on the bank, plus brokerage-side rows).")
    p("")

    # Rules, heuristics, links and the review queue (M2)
    p("## Rules (M2) — ordered, first match wins, applied before heuristics")
    p("")
    p("`fixtures/rules.json` is the machine-readable copy. Venture `Ledgerline` (fund, cap 5,000.00) must exist.")
    p("")
    p("| # | name | payee_norm contains | category | venture |")
    p("|---|---|---|---|---|")
    for i, (n, m, c, v) in enumerate(RULES, 1):
        p(f"| {i} | {n} | `{m}` | `{c}` | {v or ''} |")
    p("")
    p("Heuristics, in order, for rows no rule matched: `securities_sale` flag → `transfer.securities_sale_proceeds`;")
    p("payee contains `fee` → `debt.fees` + fee flag; `interest` on an outflow → `debt.interest` + interest flag;")
    p("`interest` on an inflow → `income.interest` + interest flag; `atm` → cash_withdrawal + needs_review, no category;")
    p("payment-app rows (Venmo export, or a bank row naming venmo/zelle/cash app/paypal) → payment_app_unknown +")
    p("needs_review, no category. Anything else stays unclassified with needs_review. Transfer and refund links run")
    p("after heuristics; a linked leg takes the transfer category and drops needs_review / payment_app_unknown.")
    p("An outflow on a firewalled account keeps needs_review until it is acknowledged (policy `firewall_exclusion`).")
    p("")
    p("### Automation outcome per row (category, why, flags) — rows that differ from their final category")
    p("")
    p("| account | posted | description | amount | category after M2 | why | flags |")
    p("|---|---|---|---:|---|---|---|")
    for r in ROWS:
        cat, why, flags = automation(r)
        final = None if r.category == "review" else r.category
        if cat != final or flags:
            p(f"| {r.account} | {r.posted} | {r.description} | {money(r.amount)} | {cat or '—'} | {why} | {', '.join(sorted(flags)) or ''} |")
    p("")
    queue = [(r, automation(r)) for r in ROWS]
    queue = [(r, a) for (r, a) in queue if "needs_review" in a[2] or a[0] is None]
    queue.sort(key=lambda t: (-abs(t[0].amount), t[0].posted))
    p(f"### Review queue after M2: {len(queue)} rows, ordered by |amount| descending")
    p("")
    p("| # | account | posted | description | amount | why |")
    p("|---|---|---|---|---:|---|")
    for i, (r, a) in enumerate(queue, 1):
        p(f"| {i} | {r.account} | {r.posted} | {r.description} | {money(r.amount)} | {a[1]} |")
    p("")
    unclassified = [r for (r, a) in queue if a[0] is None]
    p(f"Rows without a category after M2: **{len(unclassified)}** (the firewall-touch transfer leg has its category but still needs its acknowledgment).")
    p("")
    cats = {}
    for r in ROWS:
        cat, why, flags = automation(r)
        cats[cat or "—"] = cats.get(cat or "—", 0) + 1
    p("### Rows per category after M2")
    p("")
    p("| category | rows |")
    p("|---|---:|")
    for k in sorted(cats):
        p(f"| {k} | {cats[k]} |")
    p("")

    # Reconciliation (M3)
    p("## Reconciliation (M3)")
    p("")
    p("A period is `[period_start, period_end]` per account. Its opening is the account's opening balance for the first")
    p("period, else the statement closing of the last **balanced** period (roll-forward); `computed = opening + Σ posted rows`")
    p("in the period, `difference = computed − statement`, `balanced` iff the difference is exactly 0 (ADR-0021). The")
    p("hero's contributing accounts are the cash-kind accounts that are neither firewalled nor archived: "
      + ", ".join(k for k, a in ACCOUNTS.items() if a.kind in ("checking", "savings", "cash", "payment_app") and not a.firewalled) + ".")
    p("")
    p("### Every account, monthly periods (balanced fixture)")
    p("")
    p("| account | period | opening | Σ posted | computed closing | statement closing | status |")
    p("|---|---|---:|---:|---:|---:|---|")
    recon_periods = []
    for a in ACCOUNTS.values():
        prev_close = a.opening_cents
        for m in MONTHS:
            start = a.opening_date if m == "2026-07" else f"{m}-01"
            end = month_end(m)
            total = sum(r.amount for r in rows_for(a.key, m))
            computed = prev_close + total
            statement = closing(a.key, m)
            assert computed == statement
            p(f"| {a.key} | {start}..{end} | {money(prev_close)} | {money(total)} | {money(computed)} | {money(statement)} | balanced |")
            recon_periods.append({"account": a.key, "period_start": start, "period_end": end, "opening_cents": prev_close,
                                  "sum_cents": total, "computed_cents": computed, "statement_cents": statement})
            prev_close = statement
    p("")
    p("### Mutated fixture: `northbank_checking_2026-08_mutated.csv` instead of the real August file")
    p("")
    real = next(r for r in ROWS if (r.account, r.posted, r.description) == ("nbc", MUTATION[0], MUTATION[1]))
    delta = MUTATION[2] - real.amount
    aug_sum = sum(r.amount for r in rows_for("nbc", "2026-08")) + delta
    aug_computed = closing("nbc", "2026-07") + aug_sum
    aug_diff = aug_computed - closing("nbc", "2026-08")
    sep_sum = aug_sum + sum(r.amount for r in rows_for("nbc", "2026-09"))
    sep_computed = closing("nbc", "2026-07") + sep_sum
    sep_diff = sep_computed - closing("nbc", "2026-09")
    p(f"- Row {real.posted} `{real.description}` reads {money(MUTATION[2])} instead of {money(real.amount)} (delta {money(delta)}).")
    p(f"- August: opening {money(closing('nbc', '2026-07'))}, Σ {money(aug_sum)}, computed **{money(aug_computed)}** vs statement")
    p(f"  {money(closing('nbc', '2026-08'))} → difference **{money(aug_diff)}**, status `off`.")
    p(f"- September, entered next with statement {money(closing('nbc', '2026-09'))}: it rolls forward from July (the last balanced")
    p(f"  period), so its period is 2026-08-01..2026-09-30 with opening {money(closing('nbc', '2026-07'))}; computed")
    p(f"  **{money(sep_computed)}** → difference **{money(sep_diff)}**, status `off`. The difference carries until the row is fixed.")
    p("- Undoing the mutated batch and importing the real August file recomputes both periods to `balanced` in the")
    p("  same transaction; no statement is re-entered. The hero is untrusted while nbc is off, naming Northbank Checking.")
    p("")
    aug_rows = rows_for("nbc", "2026-08")
    before = [r for r in rows_for("nbc") if "2026-07-27" <= r.posted <= "2026-07-31"]
    after = [r for r in rows_for("nbc") if "2026-09-01" <= r.posted <= "2026-09-05"]
    p("### Difference explorer for the mutated August period")
    p("")
    p(f"- Rows in the period: **{len(aug_rows)}**, with a running balance from the opening; the mutated row is among them.")
    p(f"- Posted rows within 5 days before the period (2026-07-27..2026-07-31): **{len(before)}** — "
      + "; ".join(f"{r.posted} `{r.description}` {money(r.amount)}" for r in before) + ".")
    p(f"- Posted rows within 5 days after the period (2026-09-01..2026-09-05): **{len(after)}** — "
      + "; ".join(f"{r.posted} `{r.description}` {money(r.amount)}" for r in after) + ".")
    p("- Pending rows on nbc: **0**. Quarantined rows for nbc: **0** (1 when the overlap file was also imported; the explorer lists it).")
    p("")
    as_of = "2026-10-05"
    stale_days = 45
    def days_between(a: str, b: str) -> int:
        from datetime import date
        return (date.fromisoformat(b) - date.fromisoformat(a)).days
    p(f"### Trust as of {as_of} (stale window {stale_days} days, ADR-0021)")
    p("")
    p("| scenario | account | contributes | latest period end | days | status |")
    p("|---|---|---|---|---:|---|")
    contrib = {k: a.kind in ("checking", "savings", "cash", "payment_app") and not a.firewalled for k, a in ACCOUNTS.items()}
    d_sep = days_between("2026-09-30", as_of)
    d_jul = days_between("2026-07-31", as_of)
    trust_rows = []
    for k in ACCOUNTS:
        p(f"| A: every month balanced | {k} | {'yes' if contrib[k] else 'no'} | 2026-09-30 | {d_sep} | reconciled |")
        trust_rows.append({"scenario": "A", "account": k, "contributes": contrib[k], "latest_period_end": "2026-09-30", "days": d_sep, "status": "reconciled"})
    p(f"| B: mutated August on nbc, others as A | nbc | yes | 2026-09-30 (off) | {d_sep} | off |")
    trust_rows.append({"scenario": "B", "account": "nbc", "contributes": True, "latest_period_end": "2026-09-30", "days": d_sep, "status": "off"})
    p(f"| C: nbs balanced through July only, others as A | nbs | yes | 2026-07-31 | {d_jul} | stale ({d_jul} > {stale_days}) |")
    trust_rows.append({"scenario": "C", "account": "nbs", "contributes": True, "latest_period_end": "2026-07-31", "days": d_jul, "status": "stale"})
    p(f"| C with a 90-day override on nbs | nbs | yes | 2026-07-31 | {d_jul} | reconciled ({d_jul} ≤ 90) |")
    p("| D: no statement entered | any | — | — | — | never_reconciled |")
    p("")
    p("Hero trust: A → trusted. B → untrusted, naming Northbank Checking (off by "
      + money(aug_diff) + "). C → untrusted, naming Northbank Savings (stale). D → untrusted, naming every contributing account.")
    p("Cards and the firewalled brokerage never enter the hero's set, so their status marks only their own figures.")
    p("")
    global RECON_JSON
    RECON_JSON = {
        "periods": recon_periods,
        "mutated": {"file": "northbank/northbank_checking_2026-08_mutated.csv", "account": "nbc",
                    "row": {"posted": real.posted, "description": real.description, "real_cents": real.amount, "mutated_cents": MUTATION[2]},
                    "august": {"period_start": "2026-08-01", "period_end": "2026-08-31", "opening_cents": closing("nbc", "2026-07"),
                               "computed_cents": aug_computed, "statement_cents": closing("nbc", "2026-08"), "difference_cents": aug_diff},
                    "september": {"period_start": "2026-08-01", "period_end": "2026-09-30", "opening_cents": closing("nbc", "2026-07"),
                                  "computed_cents": sep_computed, "statement_cents": closing("nbc", "2026-09"), "difference_cents": sep_diff},
                    "explorer": {"in_period": len(aug_rows), "before": len(before), "after": len(after), "pending": 0, "quarantine": 0}},
        "trust": {"as_of": as_of, "stale_after_days": stale_days, "rows": trust_rows,
                  "hero_contributing": [k for k in ACCOUNTS if contrib[k]]},
    }

    # Plan inputs for M4 (defined now, numbers derived later from these definitions)
    p("## Plan inputs (defined now for M4/M5; their answers are appended at those milestones)")
    p("")
    p("- As-of date for safe-to-spend: **2026-09-30**. Timing buffer: **500.00**.")
    p("- Confirmed income: `Meridian payroll`, biweekly, anchor 2026-07-10, 3,412.77 net, deposit nbc →")
    p("  next confirmed income date **2026-10-02**.")
    p("- Confirmed obligations: rent 2,400.00 monthly day 1 (nbc); Xfinity 89.99 day 12; T-Mobile 75.00 day 18;")
    p("  ComEd variable (expected 125.00 ± 25.00) day 7; Peoples Gas (expected 40.00 ± 10.00) day 21;")
    p("  GEICO annual 1,284.00 on 09-22; Visa minimum and Amex minimum as debt minimums (M6).")
    p("- Earmarks: rent earmark funded 1,200.00 per paycheck from nbc; emergency reserve 12,000.00 in nbs.")
    p("")
    return "\n".join(out) + "\n"


def emit_rules_json() -> None:
    import json
    payload = {
        "venture": VENTURE,
        "rules": [
            {"name": n, "match_payee_contains": m, "category_code": c, "venture": v}
            for (n, m, c, v) in RULES
        ],
    }
    write("rules.json", json.dumps(payload, indent=2) + "\n")


RECON_JSON: dict = {}


def emit_recon_json() -> None:
    write("recon.json", json.dumps(RECON_JSON, indent=1) + "\n")


def emit_automation_json() -> None:
    """Per-row expected automation outcome, the machine-readable twin of the EXPECTED.md tables."""
    import json
    rows = []
    for r in ROWS:
        cat, why, flags = automation(r)
        rows.append({
            "account": r.account,
            "posted": r.posted,
            "description": r.description,
            "amount_cents": r.amount,
            "category": cat,
            "why": why,
            "flags": sorted(flags),
            "pair": r.pair,
            "refund_of": r.refund_of,
        })
    pairs = {}
    for r in ROWS:
        if r.pair:
            pairs.setdefault(r.pair, {})
            legs = pair_legs(r.pair)
            in_leg = next(l for l in legs if l.amount > 0)
            pairs[r.pair] = {"kind": "card_payment" if ACCOUNTS[in_leg.account].kind == "credit" else "internal"}
    write("automation.json", json.dumps({"rows": rows, "pairs": pairs}, indent=2) + "\n")


def main() -> None:
    files = emit_csvs()
    emit_rules_json()
    emit_automation_json()
    write("EXPECTED.md", expected_md(files))
    emit_recon_json()
    print(f"wrote {len(files)} csv files, rules.json, automation.json, recon.json and EXPECTED.md ({len(ROWS)} ledger rows)")


if __name__ == "__main__":
    main()
