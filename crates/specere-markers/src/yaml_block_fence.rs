//! YAML line-comment marker-fence for `.specify/extensions.yml`.
//!
//! HTML comments are not preserved inside YAML block sequences, so we fence
//! at the YAML line-comment level (see
//! `specs/002-phase-1-bugfix-0-2-0/contracts/extensions-mutation.md`):
//!
//! ```yaml
//! hooks:
//!   after_implement:
//!   - extension: git
//!     command: speckit.git.commit
//!     ...
//!   # >>> specere:begin claude-code-deploy
//!   - extension: specere
//!     command: specere.observe.implement
//!     ...
//!   # <<< specere:end claude-code-deploy
//! ```
//!
//! We mutate the file as text — never `serde_yaml::to_string` — so the git
//! extension's 17 pre-existing entries keep their exact formatting (required
//! for SC-004 byte-identical remove).

use super::{Error, Result};

pub fn begin_marker(unit: &str) -> String {
    format!("  # >>> specere:begin {unit}")
}

pub fn end_marker(unit: &str) -> String {
    format!("  # <<< specere:end {unit}")
}

/// Add an indented list-item hook entry inside `hooks.<verb>:`, fenced by
/// specere markers. Idempotent: no-op if a begin marker with the same unit-id
/// exists anywhere in the file. The `entry_yaml` string should be the block
/// content under a `- ` list marker, already indented two spaces below the
/// verb key (i.e. four spaces total for inner fields). Example input:
///
/// ```text
///   - extension: specere
///     command: specere.observe.implement
///     enabled: true
///     ...
/// ```
///
/// If the `hooks:` or `hooks.<verb>:` keys are missing, they are synthesized.
pub fn add(content: &str, unit: &str, verb: &str, entry_yaml: &str) -> Result<String> {
    let begin = begin_marker(unit);
    if content.lines().any(|l| l.trim_end() == begin.trim_end()) {
        return Ok(content.to_string());
    }

    let end = end_marker(unit);
    let fenced = format!("{begin}\n{}\n{end}\n", entry_yaml.trim_end_matches('\n'));

    // Strategy: find `  <verb>:` line (two-space indent, verb-key, colon).
    // Insert the fenced block at the end of that verb's list.
    let lines: Vec<&str> = content.lines().collect();
    let had_trailing_newline = content.ends_with('\n');

    let verb_key = format!("  {verb}:");
    let verb_idx = lines.iter().position(|l| l.trim_end() == verb_key);

    if let Some(vi) = verb_idx {
        // Find end of this verb's list — the last line before a non-indented
        // (or two-space-indent, non-list) line.
        let mut insert_at = lines.len();
        for (i, line) in lines.iter().enumerate().skip(vi + 1) {
            // Lines belonging to this verb's list start with at least 4 spaces
            // or are blank.
            if line.starts_with("  - ") || line.starts_with("    ") || line.is_empty() {
                continue;
            }
            insert_at = i;
            break;
        }
        // Trim trailing blanks from the verb block so we don't leave an orphan
        // blank line above the fence.
        while insert_at > vi + 1 && lines[insert_at - 1].is_empty() {
            insert_at -= 1;
        }

        let mut out_lines: Vec<String> = lines
            .iter()
            .take(insert_at)
            .map(|s| s.to_string())
            .collect();
        for l in fenced.trim_end_matches('\n').lines() {
            out_lines.push(l.to_string());
        }
        for l in lines.iter().skip(insert_at) {
            out_lines.push(l.to_string());
        }
        let mut s = out_lines.join("\n");
        if had_trailing_newline {
            s.push('\n');
        }
        return Ok(s);
    }

    // Verb missing — synthesize.
    let mut out = String::with_capacity(content.len() + fenced.len() + 32);
    let hooks_idx = lines.iter().position(|l| l.trim_end() == "hooks:");
    if let Some(hi) = hooks_idx {
        // Insert right after the hooks: line.
        for (i, l) in lines.iter().enumerate() {
            out.push_str(l);
            out.push('\n');
            if i == hi {
                out.push_str(&format!("  {verb}:\n"));
                out.push_str(&fenced);
            }
        }
    } else {
        out.push_str(content);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&format!("hooks:\n  {verb}:\n"));
        out.push_str(&fenced);
    }
    if !had_trailing_newline && out.ends_with('\n') {
        out.pop();
    }
    Ok(out)
}

/// Strip a fenced block for `unit` from the file. Returns input unchanged if
/// no begin marker is found. Raises `Unpaired` if begin is present but end is
/// missing.
pub fn remove(content: &str, unit: &str) -> Result<String> {
    let begin = begin_marker(unit);
    let end = end_marker(unit);
    let lines: Vec<&str> = content.lines().collect();

    let Some(begin_idx) = lines.iter().position(|l| l.trim_end() == begin.trim_end()) else {
        return Ok(content.to_string());
    };
    let end_idx = lines
        .iter()
        .enumerate()
        .skip(begin_idx + 1)
        .find(|(_, l)| l.trim_end() == end.trim_end())
        .map(|(i, _)| i)
        .ok_or_else(|| Error::Unpaired {
            unit: unit.to_string(),
        })?;

    let mut out: Vec<&str> = Vec::with_capacity(lines.len());
    out.extend_from_slice(&lines[..begin_idx]);
    out.extend_from_slice(&lines[end_idx + 1..]);

    // Collapse 2+ adjacent blank lines from the removal.
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

/// Validate that `content` parses as YAML. Intended for FR-P1-008 guard.
pub fn is_valid_yaml(content: &str) -> std::result::Result<(), String> {
    serde_yaml::from_str::<serde_yaml::Value>(content)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXT_BASE: &str = "installed: []\nsettings:\n  auto_execute_hooks: true\nhooks:\n  after_implement:\n  - extension: git\n    command: speckit.git.commit\n    enabled: true\n";

    #[test]
    fn add_and_remove_round_trip() {
        let entry = "  - extension: specere\n    command: specere.observe.implement\n    enabled: true\n    optional: false\n    prompt: Record observation?\n    description: specere hook\n    condition: null";
        let with_entry = add(EXT_BASE, "deploy", "after_implement", entry).unwrap();
        assert!(with_entry.contains("specere:begin deploy"));
        assert!(with_entry.contains("specere.observe.implement"));
        let stripped = remove(&with_entry, "deploy").unwrap();
        assert_eq!(stripped, EXT_BASE);
    }

    #[test]
    fn add_is_idempotent() {
        let entry = "  - extension: specere\n    command: x\n    enabled: true";
        let a = add(EXT_BASE, "d", "after_implement", entry).unwrap();
        let b = add(&a, "d", "after_implement", entry).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn remove_absent_is_noop() {
        assert_eq!(remove(EXT_BASE, "ghost").unwrap(), EXT_BASE);
    }

    #[test]
    fn yaml_validates() {
        assert!(is_valid_yaml(EXT_BASE).is_ok());
        assert!(is_valid_yaml("not: [broken").is_err());
    }
}
