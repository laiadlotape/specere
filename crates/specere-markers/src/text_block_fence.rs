//! Plain-text marker-fence for `.gitignore`-style files.
//!
//! Fence uses HTML-style comments inside a `#`-comment prefix so a line-based
//! reader (git's .gitignore parser) ignores it:
//!
//! ```text
//! # <!-- specere:begin claude-code-deploy -->
//! .claude/settings.local.json
//! # <!-- specere:end claude-code-deploy -->
//! ```
//!
//! Contract per `specs/002-phase-1-bugfix-0-2-0/contracts/extensions-mutation.md`:
//! - Add is idempotent (no-op if a begin marker with the same unit-id exists).
//! - Remove preserves all surrounding bytes; collapses adjacent blank lines to one.

use super::{Error, Result};

pub fn begin_line(unit: &str) -> String {
    format!("# <!-- specere:begin {unit} -->")
}

pub fn end_line(unit: &str) -> String {
    format!("# <!-- specere:end {unit} -->")
}

/// Insert a marker-fenced block containing `body_lines` at the tail of
/// `content`. Idempotent: if a begin marker for `unit` already exists,
/// returns the input unchanged.
pub fn add(content: &str, unit: &str, body_lines: &[&str]) -> Result<String> {
    let begin = begin_line(unit);
    if content.lines().any(|l| l == begin) {
        return Ok(content.to_string());
    }
    let mut out = String::with_capacity(content.len() + 64);
    out.push_str(content);
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out.push_str(&begin);
    out.push('\n');
    for line in body_lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&end_line(unit));
    out.push('\n');
    Ok(out)
}

/// Strip the marker-fenced block for `unit` from `content`. Returns the input
/// unchanged if no block is found. Raises `Unpaired` if the begin line exists
/// but the end line is absent.
pub fn remove(content: &str, unit: &str) -> Result<String> {
    let begin = begin_line(unit);
    let end = end_line(unit);
    let lines: Vec<&str> = content.lines().collect();

    let Some(begin_idx) = lines.iter().position(|l| *l == begin) else {
        return Ok(content.to_string());
    };
    let end_idx = lines
        .iter()
        .enumerate()
        .skip(begin_idx + 1)
        .find(|(_, l)| **l == end)
        .map(|(i, _)| i)
        .ok_or_else(|| Error::Unpaired {
            unit: unit.to_string(),
        })?;

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..begin_idx]);
    out.extend_from_slice(&lines[end_idx + 1..]);

    // Collapse 2+ adjacent blank lines (from removing block) to a single blank.
    let mut collapsed: Vec<&str> = Vec::with_capacity(out.len());
    let mut prev_blank = false;
    for line in out {
        let is_blank = line.is_empty();
        if is_blank && prev_blank {
            continue;
        }
        collapsed.push(line);
        prev_blank = is_blank;
    }
    // Trim trailing blanks.
    while collapsed.last().is_some_and(|l| l.is_empty()) {
        collapsed.pop();
    }

    let had_trailing_newline = content.ends_with('\n');
    let mut s = collapsed.join("\n");
    if !s.is_empty() && had_trailing_newline {
        s.push('\n');
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_idempotent() {
        let a = add("", "deploy", &[".claude/settings.local.json"]).unwrap();
        let b = add(&a, "deploy", &[".claude/settings.local.json"]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn roundtrip_preserves_existing_content() {
        let original = "/target\n*.log\n";
        let with_block = add(original, "deploy", &[".claude/settings.local.json"]).unwrap();
        assert!(with_block.contains("specere:begin deploy"));
        assert!(with_block.contains(".claude/settings.local.json"));
        let stripped = remove(&with_block, "deploy").unwrap();
        assert_eq!(stripped, original);
    }

    #[test]
    fn remove_on_absent_is_noop() {
        let s = "/target\n";
        assert_eq!(remove(s, "ghost").unwrap(), s);
    }

    #[test]
    fn unpaired_marker_errors() {
        let broken = "pre\n# <!-- specere:begin x -->\nbody\n";
        assert!(matches!(remove(broken, "x"), Err(Error::Unpaired { .. })));
    }
}
