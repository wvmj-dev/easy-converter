//! CSV ↔ JSON, done in Rust.
//!
//! - CSV is read as UTF-8, or as Windows-1252 when it is not valid UTF-8 (what
//!   spreadsheet programs on Windows write). The delimiter is detected when not given.
//! - CSV becomes an array of objects (first row as keys) or an array of arrays, written
//!   row by row, in the column order of the file.
//! - JSON may be an array of objects (columns are every key, in first-seen order), an
//!   array of arrays, an array of plain values or a single object. Nested values are
//!   written as JSON text.
//! - Text cells that a spreadsheet would run as a formula (`=`, `+`, `-`, `@`...) can be
//!   prefixed with `'`, so opening the CSV never runs anything.

use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use serde::de::{self, Deserializer, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;
use serde_json::Value;

use crate::error::{AppError, AppResult};
pub use crate::ops::office::decode_text as decode;

/// Largest CSV or JSON file accepted. Both are read into memory.
pub const MAX_INPUT_BYTES: u64 = 100 * 1024 * 1024;
pub const MAX_COLUMNS: usize = 10_000;
/// How often (in rows) cancellation is checked.
const CHECK_EVERY_ROWS: usize = 5_000;
const BOM: &[u8] = b"\xEF\xBB\xBF";
/// Delimiters recognised when detecting.
const CANDIDATES: [u8; 4] = *b",;\t|";
/// Lines looked at when detecting the delimiter.
const SNIFF_LINES: usize = 20;

#[derive(Debug, Clone, Copy)]
pub struct CsvToJsonOptions {
    /// `None` detects it.
    pub delimiter: Option<u8>,
    /// The first row names the columns.
    pub header: bool,
    /// Numbers and `true`/`false` become JSON numbers and booleans, empty cells `null`.
    pub typed: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct JsonToCsvOptions {
    pub delimiter: u8,
    /// Start the file with a UTF-8 byte order mark, which Excel needs to read accents.
    pub bom: bool,
    pub escape_formulas: bool,
}

fn read_limited(path: &Path) -> AppResult<Vec<u8>> {
    if fs::metadata(path)?.len() > MAX_INPUT_BYTES {
        return Err(AppError::TooLarge(format!(
            "the file can have at most {} MB",
            MAX_INPUT_BYTES / (1024 * 1024)
        )));
    }
    let mut bytes = fs::read(path)?;
    if bytes.starts_with(BOM) {
        bytes.drain(..BOM.len());
    }
    Ok(bytes)
}

/// The delimiter used by a CSV text: among `, ; tab |`, the one found the same number of
/// times on every line looked at (outside quotes), preferring the most frequent.
pub fn sniff_delimiter(text: &[u8]) -> u8 {
    let mut lines: Vec<[usize; 4]> = Vec::new();
    let mut counts = [0usize; 4];
    let mut in_quotes = false;
    let mut has_content = false;
    for &byte in text {
        match byte {
            b'"' => in_quotes = !in_quotes,
            b'\n' if !in_quotes => {
                if has_content {
                    lines.push(counts);
                    if lines.len() == SNIFF_LINES {
                        break;
                    }
                }
                counts = [0; 4];
                has_content = false;
                continue;
            }
            _ if !in_quotes => {
                if let Some(i) = CANDIDATES.iter().position(|&c| c == byte) {
                    counts[i] += 1;
                }
            }
            _ => {}
        }
        if byte != b'\r' {
            has_content = true;
        }
    }
    if has_content && lines.len() < SNIFF_LINES {
        lines.push(counts);
    }
    let score = |i: usize| -> (bool, usize) {
        let first = lines.first().map_or(0, |l| l[i]);
        let consistent = first > 0 && lines.iter().all(|l| l[i] == first);
        (consistent, lines.iter().map(|l| l[i]).sum())
    };
    (0..CANDIDATES.len())
        .filter(|&i| score(i).1 > 0)
        .max_by_key(|&i| score(i))
        .map_or(b',', |i| CANDIDATES[i])
}

/// Column names: empty ones become `column_<n>` and repeated ones get `_2`, `_3`...
fn unique_headers(record: &csv::StringRecord) -> Vec<String> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    record
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let base = if name.trim().is_empty() {
                format!("column_{}", i + 1)
            } else {
                name.to_owned()
            };
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                base
            } else {
                format!("{base}_{count}")
            }
        })
        .collect()
}

/// A cell as JSON. Typed cells only become numbers when nothing is lost: the number
/// written back must be the same text (so `007`, `1.50` and long IDs stay text).
fn cell_value(cell: &str, typed: bool) -> Value {
    if !typed {
        return Value::String(cell.to_owned());
    }
    match cell {
        "" => Value::Null,
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => {
            let starts_like_number = cell.starts_with(|c: char| c.is_ascii_digit() || c == '-');
            match serde_json::from_str::<serde_json::Number>(cell) {
                Ok(number) if starts_like_number && number.to_string() == cell => {
                    Value::Number(number)
                }
                _ => Value::String(cell.to_owned()),
            }
        }
    }
}

fn json_error(err: serde_json::Error) -> AppError {
    AppError::Internal(format!("could not write JSON: {err}"))
}

pub fn csv_to_json(
    input: &Path,
    output: &Path,
    options: CsvToJsonOptions,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    let text = decode(read_limited(input)?);
    let delimiter = options
        .delimiter
        .unwrap_or_else(|| sniff_delimiter(text.as_bytes()));
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut out = BufWriter::new(File::create(output)?);
    out.write_all(b"[")?;
    let mut headers: Option<Vec<String>> = None;
    let mut rows = 0usize;
    for (index, record) in reader.records().enumerate() {
        if index % CHECK_EVERY_ROWS == 0 {
            check()?;
        }
        let record =
            record.map_err(|err| AppError::InvalidFile(format!("unreadable CSV: {err}")))?;
        if record.len() > MAX_COLUMNS {
            return Err(AppError::TooLarge(format!(
                "a row has more than {MAX_COLUMNS} columns"
            )));
        }
        if options.header && headers.is_none() {
            headers = Some(unique_headers(&record));
            continue;
        }
        out.write_all(if rows == 0 { b"\n  " } else { b",\n  " })?;
        rows += 1;
        match &headers {
            Some(names) => {
                out.write_all(b"{")?;
                let width = names.len().max(record.len());
                for i in 0..width {
                    if i > 0 {
                        out.write_all(b", ")?;
                    }
                    let extra;
                    let key = match names.get(i) {
                        Some(name) => name.as_str(),
                        None => {
                            extra = format!("column_{}", i + 1);
                            extra.as_str()
                        }
                    };
                    serde_json::to_writer(&mut out, key).map_err(json_error)?;
                    out.write_all(b": ")?;
                    let value = record
                        .get(i)
                        .map_or(Value::Null, |cell| cell_value(cell, options.typed));
                    serde_json::to_writer(&mut out, &value).map_err(json_error)?;
                }
                out.write_all(b"}")?;
            }
            None => {
                let values: Vec<Value> = record
                    .iter()
                    .map(|cell| cell_value(cell, options.typed))
                    .collect();
                serde_json::to_writer(&mut out, &values).map_err(json_error)?;
            }
        }
    }
    out.write_all(if rows == 0 { b"]\n" } else { b"\n]\n" })?;
    out.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// JSON → CSV
// ---------------------------------------------------------------------------------------

/// One element of the top-level array. Objects keep their key order.
#[derive(Debug, PartialEq)]
enum Item {
    Object(Vec<(String, Value)>),
    Array(Vec<Value>),
    Scalar(Value),
}

/// The whole JSON file: an array of items, or a single object as one row.
#[derive(Debug)]
struct Document(Vec<Item>);

struct ItemVisitor;

impl<'de> Visitor<'de> for ItemVisitor {
    type Value = Item;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("a JSON value")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Item, A::Error> {
        let mut entries = Vec::new();
        while let Some((key, value)) = map.next_entry::<String, Value>()? {
            entries.push((key, value));
        }
        Ok(Item::Object(entries))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Item, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = seq.next_element::<Value>()? {
            values.push(value);
        }
        Ok(Item::Array(values))
    }

    fn visit_bool<E: de::Error>(self, v: bool) -> Result<Item, E> {
        Ok(Item::Scalar(Value::Bool(v)))
    }

    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Item, E> {
        Ok(Item::Scalar(Value::from(v)))
    }

    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Item, E> {
        Ok(Item::Scalar(Value::from(v)))
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Item, E> {
        Ok(Item::Scalar(Value::from(v)))
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Item, E> {
        Ok(Item::Scalar(Value::String(v.to_owned())))
    }

    fn visit_unit<E: de::Error>(self) -> Result<Item, E> {
        Ok(Item::Scalar(Value::Null))
    }
}

impl<'de> Deserialize<'de> for Item {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ItemVisitor)
    }
}

struct DocumentVisitor;

impl<'de> Visitor<'de> for DocumentVisitor {
    type Value = Document;

    fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("an array or an object")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Document, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element::<Item>()? {
            items.push(item);
        }
        Ok(Document(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Document, A::Error> {
        Ok(Document(vec![ItemVisitor.visit_map(map)?]))
    }
}

impl<'de> Deserialize<'de> for Document {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(DocumentVisitor)
    }
}

/// A value as CSV cell text.
fn cell_text(value: &Value, escape_formulas: bool) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => escape_cell(text, escape_formulas),
        Value::Bool(_) | Value::Number(_) => value.to_string(),
        Value::Array(_) | Value::Object(_) => escape_cell(&value.to_string(), escape_formulas),
    }
}

/// Text a spreadsheet would treat as a formula gets a leading `'`.
fn escape_cell(text: &str, escape_formulas: bool) -> String {
    if escape_formulas && text.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        format!("'{text}")
    } else {
        text.to_owned()
    }
}

/// The rows to write, the first being the header when there is one.
fn table(document: Document, escape_formulas: bool) -> AppResult<Vec<Vec<String>>> {
    let items = document.0;
    let objects = items
        .iter()
        .filter(|i| matches!(i, Item::Object(_)))
        .count();
    let arrays = items.iter().filter(|i| matches!(i, Item::Array(_))).count();
    let mixed = || AppError::InvalidFile("the array mixes objects, arrays and plain values".into());
    let too_wide = || AppError::TooLarge(format!("more than {MAX_COLUMNS} columns"));

    if objects > 0 {
        if objects != items.len() {
            return Err(mixed());
        }
        let mut columns: Vec<String> = Vec::new();
        let mut index: HashMap<String, usize> = HashMap::new();
        for item in &items {
            if let Item::Object(entries) = item {
                for (key, _) in entries {
                    if !index.contains_key(key) {
                        index.insert(key.clone(), columns.len());
                        columns.push(key.clone());
                        if columns.len() > MAX_COLUMNS {
                            return Err(too_wide());
                        }
                    }
                }
            }
        }
        let mut rows = Vec::with_capacity(items.len() + 1);
        rows.push(
            columns
                .iter()
                .map(|c| escape_cell(c, escape_formulas))
                .collect(),
        );
        for item in items {
            if let Item::Object(entries) = item {
                let mut row = vec![String::new(); columns.len()];
                for (key, value) in entries {
                    if let Some(&i) = index.get(&key) {
                        row[i] = cell_text(&value, escape_formulas);
                    }
                }
                rows.push(row);
            }
        }
        return Ok(rows);
    }
    if arrays > 0 {
        if arrays != items.len() {
            return Err(mixed());
        }
        return items
            .into_iter()
            .map(|item| match item {
                Item::Array(values) if values.len() <= MAX_COLUMNS => Ok(values
                    .iter()
                    .map(|v| cell_text(v, escape_formulas))
                    .collect()),
                _ => Err(too_wide()),
            })
            .collect();
    }
    let mut rows = vec![vec!["value".to_owned()]];
    rows.extend(items.into_iter().map(|item| match item {
        Item::Scalar(value) => vec![cell_text(&value, escape_formulas)],
        _ => Vec::new(),
    }));
    Ok(rows)
}

pub fn json_to_csv(
    input: &Path,
    output: &Path,
    options: JsonToCsvOptions,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    let bytes = read_limited(input)?;
    let document: Document = serde_json::from_slice(&bytes)
        .map_err(|err| AppError::InvalidFile(format!("unreadable JSON: {err}")))?;
    drop(bytes);
    check()?;
    let rows = table(document, options.escape_formulas)?;
    let mut file = BufWriter::new(File::create(output)?);
    if options.bom {
        file.write_all(BOM)?;
    }
    let mut writer = csv::WriterBuilder::new()
        .delimiter(options.delimiter)
        .flexible(true)
        .from_writer(file);
    for (index, row) in rows.iter().enumerate() {
        if index % CHECK_EVERY_ROWS == 0 {
            check()?;
        }
        writer
            .write_record(row)
            .map_err(|err| AppError::Internal(format!("could not write CSV: {err}")))?;
    }
    writer.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok() -> AppResult<()> {
        Ok(())
    }

    struct Scratch(std::path::PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ec-csvjson-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn detects_delimiters() {
        assert_eq!(sniff_delimiter(b"a,b,c\n1,2,3\n"), b',');
        assert_eq!(sniff_delimiter(b"nome;valor\nAna;1,5\nBia;2,25\n"), b';');
        assert_eq!(sniff_delimiter(b"a\tb\r\n1\t2\r\n"), b'\t');
        assert_eq!(sniff_delimiter(b"\"x;y\",b\n\"1;2\",3"), b',');
        assert_eq!(sniff_delimiter(b"single column\n"), b',');
        assert_eq!(sniff_delimiter(b""), b',');
    }

    #[test]
    fn decodes_windows_1252() {
        assert_eq!(decode(b"Jo\xe3o \x80 \x93x\x94".to_vec()), "João € “x”");
        assert_eq!(decode("ção".as_bytes().to_vec()), "ção");
    }

    #[test]
    fn typed_cells_never_lose_information() {
        assert_eq!(cell_value("42", true), Value::from(42));
        assert_eq!(cell_value("-1.5", true), serde_json::json!(-1.5));
        assert_eq!(cell_value("", true), Value::Null);
        assert_eq!(cell_value("true", true), Value::Bool(true));
        for text in [
            "007",
            "1.50",
            "1e5",
            "12345678901234567890123",
            " 1",
            "NaN",
            "+1",
        ] {
            assert_eq!(cell_value(text, true), Value::String(text.into()), "{text}");
        }
        assert_eq!(cell_value("42", false), Value::String("42".into()));
    }

    #[test]
    fn csv_becomes_objects_in_column_order() {
        let dir = Scratch::new("objects");
        let input = dir.0.join("in.csv");
        let output = dir.0.join("out.json");
        fs::write(
            &input,
            b"\xEF\xBB\xBFzeta;alfa;;alfa\nJo\xc3\xa3o;1;x\n\"a;b\";2;y;z;extra\n",
        )
        .unwrap();
        let options = CsvToJsonOptions {
            delimiter: None,
            header: true,
            typed: true,
        };
        csv_to_json(&input, &output, options, &ok).unwrap();
        let text = fs::read_to_string(&output).unwrap();
        assert!(text.find("\"zeta\"").unwrap() < text.find("\"alfa\"").unwrap());
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                {"zeta": "João", "alfa": 1, "column_3": "x", "alfa_2": null},
                {"zeta": "a;b", "alfa": 2, "column_3": "y", "alfa_2": "z", "column_5": "extra"}
            ])
        );
    }

    #[test]
    fn csv_becomes_arrays_without_header() {
        let dir = Scratch::new("arrays");
        let input = dir.0.join("in.csv");
        let output = dir.0.join("out.json");
        fs::write(&input, "a,b\n1,2\n").unwrap();
        let options = CsvToJsonOptions {
            delimiter: Some(b','),
            header: false,
            typed: false,
        };
        csv_to_json(&input, &output, options, &ok).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&output).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!([["a", "b"], ["1", "2"]]));

        fs::write(&input, "").unwrap();
        csv_to_json(&input, &output, options, &ok).unwrap();
        assert_eq!(fs::read_to_string(&output).unwrap(), "[]\n");
    }

    #[test]
    fn json_objects_become_csv_with_every_key() {
        let dir = Scratch::new("tocsv");
        let input = dir.0.join("in.json");
        let output = dir.0.join("out.csv");
        fs::write(
            &input,
            r#"[{"z": 1, "a": "=HYPERLINK(\"x\")", "n": null},
                {"a": "ok;semi", "extra": {"k": [1, 2]}, "z": true}]"#,
        )
        .unwrap();
        let options = JsonToCsvOptions {
            delimiter: b';',
            bom: true,
            escape_formulas: true,
        };
        json_to_csv(&input, &output, options, &ok).unwrap();
        let bytes = fs::read(&output).unwrap();
        assert!(bytes.starts_with(BOM));
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "z;a;n;extra");
        assert_eq!(lines[1], "1;\"'=HYPERLINK(\"\"x\"\")\";;");
        assert_eq!(lines[2], "true;\"ok;semi\";;\"{\"\"k\"\":[1,2]}\"");
    }

    #[test]
    fn json_arrays_scalars_and_single_objects() {
        let dir = Scratch::new("shapes");
        let input = dir.0.join("in.json");
        let output = dir.0.join("out.csv");
        let options = JsonToCsvOptions {
            delimiter: b',',
            bom: false,
            escape_formulas: false,
        };
        let cases = [
            (r#"[[1, "-2"], ["a"]]"#, "1,-2\na\n"),
            (r#"[1, "x", null]"#, "value\n1\nx\n\"\"\n"),
            (r#"{"b": 1, "a": 2}"#, "b,a\n1,2\n"),
        ];
        for (json, expected) in cases {
            fs::write(&input, json).unwrap();
            json_to_csv(&input, &output, options, &ok).unwrap();
            assert_eq!(fs::read_to_string(&output).unwrap(), expected, "{json}");
        }
        for bad in [r#"[{"a": 1}, [1]]"#, "not json", "42"] {
            fs::write(&input, bad).unwrap();
            assert!(
                matches!(
                    json_to_csv(&input, &output, options, &ok),
                    Err(AppError::InvalidFile(_))
                ),
                "{bad}"
            );
        }
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = Scratch::new("cancel");
        let input = dir.0.join("in.csv");
        fs::write(&input, "a\n1\n").unwrap();
        let cancelled = || Err(AppError::Cancelled);
        let options = CsvToJsonOptions {
            delimiter: None,
            header: true,
            typed: false,
        };
        assert!(matches!(
            csv_to_json(&input, &dir.0.join("o.json"), options, &cancelled),
            Err(AppError::Cancelled)
        ));
    }
}
