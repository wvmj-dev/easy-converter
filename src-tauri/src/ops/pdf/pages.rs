//! Page selections: lists of 1-based, inclusive ranges such as `1-3, 5, 9-7`.
//!
//! A range whose start is after its end selects the pages in reverse order, so a selection
//! can also reorder pages. The frontend parses what the user types; this module validates
//! the result and turns it into qpdf syntax or page indices.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{AppError, AppResult};

/// Highest page number accepted in a selection.
pub const MAX_PAGE: u32 = 100_000;
/// Most ranges in one selection.
pub const MAX_RANGES: usize = 1_000;

/// Pages from `start` to `end`, both included and counted from 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct PageRange {
    pub start: u32,
    pub end: u32,
}

impl PageRange {
    pub fn single(page: u32) -> Self {
        Self {
            start: page,
            end: page,
        }
    }

    fn len(self) -> u64 {
        u64::from(self.start.abs_diff(self.end)) + 1
    }
}

/// Checks that a selection is non-empty and within the limits.
pub fn validate(ranges: &[PageRange]) -> AppResult<()> {
    if ranges.is_empty() {
        return Err(AppError::InvalidParameter("no pages selected".into()));
    }
    if ranges.len() > MAX_RANGES {
        return Err(AppError::InvalidParameter(format!(
            "at most {MAX_RANGES} page ranges"
        )));
    }
    for range in ranges {
        for page in [range.start, range.end] {
            if !(1..=MAX_PAGE).contains(&page) {
                return Err(AppError::InvalidParameter(format!(
                    "page numbers must be between 1 and {MAX_PAGE}"
                )));
            }
        }
    }
    let total: u64 = ranges.iter().map(|r| r.len()).sum();
    if total > u64::from(MAX_PAGE) {
        return Err(AppError::InvalidParameter(format!(
            "at most {MAX_PAGE} pages can be selected"
        )));
    }
    Ok(())
}

/// The selection in qpdf's page range syntax (`1-3,5,9-7`).
pub fn to_qpdf(ranges: &[PageRange]) -> String {
    ranges
        .iter()
        .map(|r| {
            if r.start == r.end {
                r.start.to_string()
            } else {
                format!("{}-{}", r.start, r.end)
            }
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// Zero-based page indices in selection order, checked against the document length.
/// `None` selects every page.
pub fn indices(ranges: Option<&[PageRange]>, page_count: u32) -> AppResult<Vec<u32>> {
    let Some(ranges) = ranges else {
        return Ok((0..page_count).collect());
    };
    validate(ranges)?;
    let mut out = Vec::new();
    for range in ranges {
        let highest = range.start.max(range.end);
        if highest > page_count {
            return Err(AppError::InvalidParameter(format!(
                "page {highest} does not exist; the document has {page_count} pages"
            )));
        }
        if range.start <= range.end {
            out.extend(range.start - 1..range.end);
        } else {
            out.extend((range.end - 1..range.start).rev());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: u32, end: u32) -> PageRange {
        PageRange { start, end }
    }

    #[test]
    fn formats_for_qpdf() {
        assert_eq!(to_qpdf(&[r(1, 3), r(5, 5), r(9, 7)]), "1-3,5,9-7");
    }

    #[test]
    fn expands_in_order_including_reversed_ranges() {
        assert_eq!(
            indices(Some(&[r(3, 3), r(1, 2), r(6, 4)]), 6).unwrap(),
            vec![2, 0, 1, 5, 4, 3]
        );
        assert_eq!(indices(None, 3).unwrap(), vec![0, 1, 2]);
    }

    #[test]
    fn rejects_bad_selections() {
        assert!(validate(&[]).is_err());
        assert!(validate(&[r(0, 2)]).is_err());
        assert!(validate(&[r(1, MAX_PAGE + 1)]).is_err());
        assert!(validate(&vec![r(1, 1); MAX_RANGES + 1]).is_err());
        assert!(validate(&[r(1, MAX_PAGE), r(1, 2)]).is_err());
        assert!(indices(Some(&[r(2, 4)]), 3).is_err());
        validate(&[r(1, MAX_PAGE)]).unwrap();
    }
}
