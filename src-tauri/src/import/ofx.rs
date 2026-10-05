//! OFX/QFX statements (ARCHITECTURE §6.1, ADR-0046): the SGML form (OFX 1.x, `OFXHEADER:100`,
//! leaf tags left open) and the XML form (OFX 2.x, every tag closed), read by one tolerant
//! tokenizer. `FITID` is the row's external id, `<LEDGERBAL>` is the statement closing a
//! reconciliation can start from. The whole file is parsed before anything is written.

use std::collections::BTreeMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::dates::{format_civil, CivilDate};
use crate::error::{AppError, AppResult};
use crate::import::csv::{flag_bit, FileClosing, ParsedFile, ParsedRow, RowStatus};
use crate::import::profile::OfxSpec;
use crate::money::parse_decimal_cents;

/// What the file says about itself, shown on the import screen beside the mapped rows.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct OfxInfo {
    /// `sgml` or `xml`.
    pub form: String,
    pub bank_id: String,
    pub acct_id: String,
    pub acct_type: String,
    pub currency: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub rows: usize,
}

/// True when the bytes carry an OFX document: an `OFXHEADER` line, an `<?OFX` declaration or an
/// `<OFX>` root within the first kilobytes.
pub fn is_ofx(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_uppercase();
    head.contains("OFXHEADER") || head.contains("<?OFX") || head.contains("<OFX>")
}

#[derive(Debug, PartialEq, Eq)]
enum Token {
    /// `<NAME>` with the text that follows it up to the next tag, trimmed.
    Open(String, String),
    Close(String),
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest.find(';') else {
            out.push_str(rest);
            return out;
        };
        let entity = &rest[1..end];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" => out.push('\''),
            "nbsp" => out.push(' '),
            _ => {
                let code = entity
                    .strip_prefix('#')
                    .and_then(|n| n.parse::<u32>().ok())
                    .and_then(char::from_u32);
                match code {
                    Some(c) => out.push(c),
                    None => out.push_str(&rest[..=end]),
                }
            }
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Tokenize the document body (everything from the first `<OFX>` on).
fn tokenize(text: &str) -> Vec<Token> {
    let upper = text.to_ascii_uppercase();
    let start = upper.find("<OFX>").unwrap_or(0);
    let body = &text[start..];
    let mut tokens = Vec::new();
    let mut rest = body;
    while let Some(lt) = rest.find('<') {
        rest = &rest[lt + 1..];
        let Some(gt) = rest.find('>') else {
            break;
        };
        let tag = rest[..gt].trim();
        rest = &rest[gt + 1..];
        if tag.starts_with('?') || tag.starts_with('!') {
            continue;
        }
        if let Some(name) = tag.strip_prefix('/') {
            tokens.push(Token::Close(name.trim().to_ascii_uppercase()));
            continue;
        }
        let value_end = rest.find('<').unwrap_or(rest.len());
        let value = decode_entities(rest[..value_end].trim());
        tokens.push(Token::Open(tag.to_ascii_uppercase(), value));
    }
    tokens
}

fn parse_error(row: usize, column: &str, message: impl Into<String>) -> AppError {
    AppError::Parse {
        row,
        column: column.to_string(),
        message: message.into(),
    }
}

/// `YYYYMMDD` with an optional time, fraction and zone suffix (`20260815120000.000[-5:EST]`);
/// the civil date is the first eight digits, as the bank states it.
fn parse_ofx_date(value: &str, row: usize, column: &str) -> AppResult<CivilDate> {
    let digits: String = value.chars().take(8).collect();
    if digits.len() != 8 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(parse_error(
            row,
            column,
            format!("{value:?} is not an OFX date (YYYYMMDD…)"),
        ));
    }
    NaiveDate::parse_from_str(&digits, "%Y%m%d")
        .map_err(|e| parse_error(row, column, format!("{value:?}: {e}")))
}

fn parse_amount(value: &str, row: usize, column: &str) -> AppResult<i64> {
    parse_decimal_cents(value).map_err(|e| parse_error(row, column, format!("{value:?}: {e}")))
}

#[derive(Default)]
struct Header {
    bank_id: Option<String>,
    acct_id: Option<String>,
    acct_type: Option<String>,
    currency: Option<String>,
    start: Option<String>,
    end: Option<String>,
    ledger_amount: Option<String>,
    ledger_date: Option<String>,
    accounts_seen: usize,
}

/// Parse an OFX/QFX document into ledger rows. Every `STMTTRN` needs `DTPOSTED`, `TRNAMT` and
/// `FITID`; `NAME` (or `PAYEE/NAME`) is the payee, `MEMO` the memo, `DTUSER` the effective date.
/// Rows are posted (OFX carries no pending state). `CURDEF` other than USD is refused.
pub fn parse(bytes: &[u8], spec: &OfxSpec) -> AppResult<(ParsedFile, OfxInfo)> {
    if !is_ofx(bytes) {
        return Err(AppError::validation(
            "file",
            "not an OFX/QFX file: no OFXHEADER line or <OFX> element",
        ));
    }
    let text = String::from_utf8_lossy(bytes);
    let form = if text.trim_start().starts_with("<?xml") || text.contains("<?OFX") {
        "xml"
    } else {
        "sgml"
    };
    let tokens = tokenize(&text);
    let flags_by_type: BTreeMap<String, u32> = spec
        .flags_by_trntype
        .iter()
        .map(|(k, names)| {
            let mut bits = 0u32;
            for name in names {
                bits |= flag_bit(name).ok_or_else(|| {
                    AppError::validation(
                        "spec_json",
                        format!("unknown flag {name:?} in flags_by_trntype"),
                    )
                })?;
            }
            Ok((k.to_ascii_uppercase(), bits))
        })
        .collect::<AppResult<_>>()?;

    let mut header = Header::default();
    let mut rows: Vec<ParsedRow> = Vec::new();
    let mut field_order: Vec<String> = Vec::new();
    let mut in_trn: Option<BTreeMap<String, String>> = None;
    let mut in_ledgerbal = false;
    let mut in_acct_from = false;
    for token in tokens {
        match token {
            Token::Open(name, value) => {
                if let Some(fields) = in_trn.as_mut() {
                    if !value.is_empty() {
                        if !field_order.contains(&name) {
                            field_order.push(name.clone());
                        }
                        fields.entry(name).or_insert(value);
                    }
                    continue;
                }
                match name.as_str() {
                    "STMTTRN" => in_trn = Some(BTreeMap::new()),
                    "LEDGERBAL" => in_ledgerbal = true,
                    "BANKACCTFROM" | "CCACCTFROM" => {
                        in_acct_from = true;
                        header.accounts_seen += 1;
                    }
                    "CURDEF" => {
                        header.currency.get_or_insert(value);
                    }
                    "DTSTART" => {
                        header.start.get_or_insert(value);
                    }
                    "DTEND" => {
                        header.end.get_or_insert(value);
                    }
                    "BANKID" if in_acct_from => {
                        header.bank_id.get_or_insert(value);
                    }
                    "ACCTID" if in_acct_from => {
                        header.acct_id.get_or_insert(value);
                    }
                    "ACCTTYPE" if in_acct_from => {
                        header.acct_type.get_or_insert(value);
                    }
                    "BALAMT" if in_ledgerbal => {
                        header.ledger_amount.get_or_insert(value);
                    }
                    "DTASOF" if in_ledgerbal => {
                        header.ledger_date.get_or_insert(value);
                    }
                    _ => {}
                }
            }
            Token::Close(name) => match name.as_str() {
                "STMTTRN" => {
                    if let Some(fields) = in_trn.take() {
                        rows.push(row_from(&fields, rows.len() + 1, &flags_by_type)?);
                    }
                }
                "LEDGERBAL" => in_ledgerbal = false,
                "BANKACCTFROM" | "CCACCTFROM" => in_acct_from = false,
                _ => {}
            },
        }
    }
    if in_trn.is_some() {
        return Err(parse_error(
            rows.len() + 1,
            "STMTTRN",
            "the last transaction is not closed",
        ));
    }
    if header.accounts_seen > 1 {
        return Err(AppError::Unsupported(format!(
            "the file holds {} accounts; export one account at a time",
            header.accounts_seen
        )));
    }
    let currency = header.currency.unwrap_or_else(|| "USD".to_string());
    if !currency.eq_ignore_ascii_case("USD") {
        return Err(AppError::Unsupported(format!(
            "the statement is in {currency}; Kept v1 handles USD only"
        )));
    }
    let closing = match (header.ledger_amount, header.ledger_date) {
        (Some(amount), Some(date)) => Some(FileClosing {
            date: parse_ofx_date(&date, 0, "DTASOF")?,
            cents: parse_amount(&amount, 0, "BALAMT")?,
        }),
        (Some(amount), None) => Some(FileClosing {
            date: match &header.end {
                Some(end) => parse_ofx_date(end, 0, "DTEND")?,
                None => rows
                    .iter()
                    .map(|r| r.posted_date)
                    .max()
                    .ok_or_else(|| parse_error(0, "LEDGERBAL", "a balance without a date"))?,
            },
            cents: parse_amount(&amount, 0, "BALAMT")?,
        }),
        _ => None,
    };
    let info = OfxInfo {
        form: form.to_string(),
        bank_id: header.bank_id.unwrap_or_default(),
        acct_id: header.acct_id.unwrap_or_default(),
        acct_type: header.acct_type.unwrap_or_default(),
        currency,
        start: header
            .start
            .map(|s| parse_ofx_date(&s, 0, "DTSTART").map(format_civil))
            .transpose()?,
        end: header
            .end
            .map(|s| parse_ofx_date(&s, 0, "DTEND").map(format_civil))
            .transpose()?,
        rows: rows.len(),
    };
    Ok((
        ParsedFile {
            header: field_order,
            rows,
            blank_rows: 0,
            closing,
        },
        info,
    ))
}

fn row_from(
    fields: &BTreeMap<String, String>,
    row: usize,
    flags_by_type: &BTreeMap<String, u32>,
) -> AppResult<ParsedRow> {
    let need = |key: &str| -> AppResult<&String> {
        fields
            .get(key)
            .ok_or_else(|| parse_error(row, key, "the transaction has no such element"))
    };
    let posted_date = parse_ofx_date(need("DTPOSTED")?, row, "DTPOSTED")?;
    let effective_date = match fields.get("DTUSER") {
        Some(v) => parse_ofx_date(v, row, "DTUSER")?,
        None => posted_date,
    };
    let amount_cents = parse_amount(need("TRNAMT")?, row, "TRNAMT")?;
    let external_id = need("FITID")?.trim().to_string();
    if external_id.is_empty() {
        return Err(parse_error(row, "FITID", "the transaction id is empty"));
    }
    let mut payee_raw = fields.get("NAME").cloned().unwrap_or_default();
    if payee_raw.is_empty() {
        if let Some(check) = fields.get("CHECKNUM") {
            payee_raw = format!("CHECK {check}");
        }
    }
    let memo = fields.get("MEMO").cloned().unwrap_or_default();
    let trntype = fields
        .get("TRNTYPE")
        .map(|t| t.to_ascii_uppercase())
        .unwrap_or_default();
    let flags = flags_by_type.get(&trntype).copied().unwrap_or(0);
    Ok(ParsedRow {
        row,
        posted_date,
        effective_date,
        amount_cents,
        payee_raw,
        memo,
        status: RowStatus::Posted,
        external_id: Some(external_id),
        balance_cents: None,
        flags,
        skipped: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::import::csv::{FLAG_CASH_WITHDRAWAL, FLAG_FEE, FLAG_NEEDS_REVIEW};

    fn spec() -> OfxSpec {
        OfxSpec::parse(
            r#"{"flags_by_trntype":{"ATM":["cash_withdrawal","needs_review"],"FEE":["fee"]}}"#,
        )
        .unwrap()
    }

    const SGML: &str = "OFXHEADER:100\r\nDATA:OFXSGML\r\nVERSION:102\r\n\r\n<OFX><SIGNONMSGSRSV1><SONRS><STATUS><CODE>0<SEVERITY>INFO</STATUS></SONRS></SIGNONMSGSRSV1>\r\n<BANKMSGSRSV1><STMTTRNRS><STMTRS><CURDEF>USD<BANKACCTFROM><BANKID>1<ACCTID>99<ACCTTYPE>CHECKING</BANKACCTFROM>\r\n<BANKTRANLIST><DTSTART>20260801<DTEND>20260831\r\n<STMTTRN><TRNTYPE>ATM<DTPOSTED>20260815120000.000[-5:CDT]<DTUSER>20260814<TRNAMT>-163.42<FITID>A1<NAME>ATM WITHDRAWAL<MEMO>CDMX</STMTTRN>\r\n<STMTTRN><TRNTYPE>FEE<DTPOSTED>20260815<TRNAMT>-3.00<FITID>A2<NAME>FEE &amp; CHARGE</STMTTRN>\r\n<STMTTRN><TRNTYPE>CHECK<DTPOSTED>20260816<TRNAMT>-20.00<FITID>A3<CHECKNUM>1042</STMTTRN>\r\n</BANKTRANLIST><LEDGERBAL><BALAMT>1422.66<DTASOF>20260831</LEDGERBAL><AVAILBAL><BALAMT>1400.00<DTASOF>20260831</AVAILBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>\r\n";

    #[test]
    fn sgml_rows_dates_amounts_flags_and_ledger_balance() {
        let (file, info) = parse(SGML.as_bytes(), &spec()).unwrap();
        assert_eq!(info.form, "sgml");
        assert_eq!(info.acct_id, "99");
        assert_eq!(info.acct_type, "CHECKING");
        assert_eq!(info.start.as_deref(), Some("2026-08-01"));
        assert_eq!(file.rows.len(), 3);
        let r = &file.rows[0];
        assert_eq!(format_civil(r.posted_date), "2026-08-15");
        assert_eq!(format_civil(r.effective_date), "2026-08-14");
        assert_eq!(r.amount_cents, -16_342);
        assert_eq!(r.external_id.as_deref(), Some("A1"));
        assert_eq!(r.payee_raw, "ATM WITHDRAWAL");
        assert_eq!(r.memo, "CDMX");
        assert_eq!(r.flags, FLAG_CASH_WITHDRAWAL | FLAG_NEEDS_REVIEW);
        assert_eq!(file.rows[1].payee_raw, "FEE & CHARGE");
        assert_eq!(file.rows[1].flags, FLAG_FEE);
        assert_eq!(file.rows[2].payee_raw, "CHECK 1042");
        let closing = file.closing.unwrap();
        assert_eq!(closing.cents, 142_266);
        assert_eq!(format_civil(closing.date), "2026-08-31");
        assert_eq!(
            file.header,
            vec!["TRNTYPE", "DTPOSTED", "DTUSER", "TRNAMT", "FITID", "NAME", "MEMO", "CHECKNUM"]
        );
    }

    #[test]
    fn xml_form_parses_the_same_and_rejects_other_currencies() {
        let xml = r#"<?xml version="1.0"?><?OFX OFXHEADER="200" VERSION="220"?><OFX><BANKMSGSRSV1><STMTTRNRS><STMTRS><CURDEF>USD</CURDEF><BANKACCTFROM><BANKID>1</BANKID><ACCTID>5</ACCTID><ACCTTYPE>SAVINGS</ACCTTYPE></BANKACCTFROM><BANKTRANLIST><STMTTRN><TRNTYPE>CREDIT</TRNTYPE><DTPOSTED>20260903</DTPOSTED><TRNAMT>500.00</TRNAMT><FITID>N1</FITID><NAME>TRANSFER</NAME></STMTTRN></BANKTRANLIST><LEDGERBAL><BALAMT>13531.99</BALAMT><DTASOF>20260930</DTASOF></LEDGERBAL></STMTRS></STMTTRNRS></BANKMSGSRSV1></OFX>"#;
        let (file, info) = parse(xml.as_bytes(), &spec()).unwrap();
        assert_eq!(info.form, "xml");
        assert_eq!(file.rows.len(), 1);
        assert_eq!(file.rows[0].amount_cents, 50_000);
        assert_eq!(file.rows[0].status, RowStatus::Posted);
        assert_eq!(file.closing.unwrap().cents, 1_353_199);
        let eur = xml.replace("<CURDEF>USD</CURDEF>", "<CURDEF>EUR</CURDEF>");
        assert!(matches!(
            parse(eur.as_bytes(), &spec()),
            Err(AppError::Unsupported(_))
        ));
        assert!(!is_ofx(b"Date,Description,Amount\n2026-07-01,RENT,-2400\n"));
        assert!(matches!(
            parse(b"Date,Description,Amount\n", &spec()),
            Err(AppError::Validation { .. })
        ));
    }

    #[test]
    fn a_transaction_without_an_id_or_date_names_its_row_and_element() {
        let bad = SGML.replace("<FITID>A2", "<FITID>");
        let err = parse(bad.as_bytes(), &spec()).err().unwrap();
        assert!(
            matches!(err, AppError::Parse { row: 2, ref column, .. } if column == "FITID"),
            "{err:?}"
        );
        let bad = SGML.replace("<DTPOSTED>20260816", "<DTPOSTED>2026-08-16");
        let err = parse(bad.as_bytes(), &spec()).err().unwrap();
        assert!(
            matches!(err, AppError::Parse { row: 3, ref column, .. } if column == "DTPOSTED"),
            "{err:?}"
        );
    }
}
