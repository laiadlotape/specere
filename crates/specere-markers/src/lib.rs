//! Marker-fenced shared-file editing. Every file SpecERE co-owns (`CLAUDE.md`,
//! `AGENTS.md`, `.envrc`, etc.) has SpecERE content fenced by a pair of HTML
//! comments so we can round-trip cleanly and never touch user content.
//!
//! The fence pair uses the unit id and an optional block id:
//!
//! ```text
//! <!-- specere:begin speckit -->
//! ... SpecERE-owned content ...
//! <!-- specere:end   speckit -->
//! ```

use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("begin marker for unit `{unit}` not found")]
    BeginMissing { unit: String },
    #[error("end marker for unit `{unit}` not found after begin")]
    EndMissing { unit: String },
    #[error("multiple begin markers for unit `{unit}`")]
    Duplicate { unit: String },
    #[error("marker pair for unit `{unit}` is unpaired")]
    Unpaired { unit: String },
}

pub type Result<T> = std::result::Result<T, Error>;

pub mod text_block_fence;
pub mod yaml_block_fence;

pub fn begin_line(unit: &str, block: Option<&str>) -> String {
    match block {
        Some(b) => format!("<!-- specere:begin {} {} -->", unit, b),
        None => format!("<!-- specere:begin {} -->", unit),
    }
}

pub fn end_line(unit: &str, block: Option<&str>) -> String {
    match block {
        Some(b) => format!("<!-- specere:end {} {} -->", unit, b),
        None => format!("<!-- specere:end {} -->", unit),
    }
}

/// Insert or replace a marker-fenced block inside `content`. Returns the new
/// content. If the markers are absent, the block is appended at the end.
pub fn upsert_block(content: &str, unit: &str, block: Option<&str>, body: &str) -> Result<String> {
    let begin = begin_line(unit, block);
    let end = end_line(unit, block);

    let begins: Vec<_> = content.match_indices(&begin).collect();
    if begins.len() > 1 {
        return Err(Error::Duplicate {
            unit: unit.to_string(),
        });
    }

    if let Some((start_idx, _)) = begins.first() {
        let after_begin = start_idx + begin.len();
        let rest = &content[after_begin..];
        let end_rel = rest.find(&end).ok_or_else(|| Error::EndMissing {
            unit: unit.to_string(),
        })?;
        let end_abs = after_begin + end_rel + end.len();

        let mut out = String::with_capacity(content.len() + body.len());
        out.push_str(&content[..*start_idx]);
        out.push_str(&begin);
        out.push('\n');
        out.push_str(body.trim_end_matches('\n'));
        out.push('\n');
        out.push_str(&end);
        out.push_str(&content[end_abs..]);
        Ok(out)
    } else {
        let mut out = String::from(content);
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&begin);
        out.push('\n');
        out.push_str(body.trim_end_matches('\n'));
        out.push('\n');
        out.push_str(&end);
        out.push('\n');
        Ok(out)
    }
}

/// Strip a marker-fenced block. If the markers are absent, returns `content`
/// unchanged.
pub fn strip_block(content: &str, unit: &str, block: Option<&str>) -> Result<String> {
    let begin = begin_line(unit, block);
    let end = end_line(unit, block);

    let Some(start_idx) = content.find(&begin) else {
        return Ok(content.to_string());
    };
    let after_begin = start_idx + begin.len();
    let rest = &content[after_begin..];
    let end_rel = rest.find(&end).ok_or_else(|| Error::EndMissing {
        unit: unit.to_string(),
    })?;
    let end_abs = after_begin + end_rel + end.len();

    let mut out = String::with_capacity(content.len());
    out.push_str(&content[..start_idx]);
    let tail = &content[end_abs..];
    let tail = tail.strip_prefix('\n').unwrap_or(tail);
    out.push_str(tail);

    let trimmed = out.trim_end_matches('\n').to_string();
    if !trimmed.is_empty() {
        Ok(format!("{trimmed}\n"))
    } else {
        Ok(trimmed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_append_and_strip() {
        let original = "# Title\n\nSome user content.\n";
        let body = "SpecERE-owned text.";
        let with_block = upsert_block(original, "speckit", None, body).unwrap();
        assert!(with_block.contains("<!-- specere:begin speckit -->"));
        assert!(with_block.contains("SpecERE-owned text."));
        let stripped = strip_block(&with_block, "speckit", None).unwrap();
        assert_eq!(stripped, original);
    }

    #[test]
    fn upsert_replaces_existing_body() {
        let original = "pre\n<!-- specere:begin x -->\nold body\n<!-- specere:end x -->\npost\n";
        let replaced = upsert_block(original, "x", None, "new body").unwrap();
        assert!(replaced.contains("new body"));
        assert!(!replaced.contains("old body"));
    }

    #[test]
    fn strip_is_idempotent_when_markers_absent() {
        let original = "nothing here\n";
        let out = strip_block(original, "ghost", None).unwrap();
        assert_eq!(out, original);
    }
}
