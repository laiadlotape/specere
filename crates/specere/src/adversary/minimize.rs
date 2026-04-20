//! FR-EQ-023 — delta-debug minimization of counter-example input.
//!
//! Classic ddmin on line-split input: we halve, test, discard or recurse
//! until no further shrink reproduces the failure. `predicate` returns
//! `true` if the candidate still witnesses the failure.
//!
//! Time-boxed: if `deadline` elapses, we return the best shrink so far
//! (FR-EQ-023: "if it doesn't converge, record the original").

use std::time::Instant;

pub fn minimize<F>(original: &str, mut predicate: F, deadline: Instant) -> String
where
    F: FnMut(&str) -> bool,
{
    let lines: Vec<&str> = original.lines().collect();
    if lines.is_empty() {
        return original.to_string();
    }
    if !predicate(original) {
        return original.to_string();
    }
    let mut current: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let mut n = 2usize;
    while current.len() >= 2 {
        if Instant::now() >= deadline {
            break;
        }
        let chunk = current.len().div_ceil(n).max(1);
        let mut made_progress = false;
        let mut i = 0;
        while i < current.len() {
            if Instant::now() >= deadline {
                break;
            }
            let end = (i + chunk).min(current.len());
            let mut candidate: Vec<String> = Vec::with_capacity(current.len() - (end - i));
            candidate.extend_from_slice(&current[..i]);
            candidate.extend_from_slice(&current[end..]);
            if candidate.is_empty() {
                i = end;
                continue;
            }
            let joined = candidate.join("\n");
            if predicate(&joined) {
                current = candidate;
                made_progress = true;
            } else {
                i = end;
            }
        }
        if !made_progress {
            if n >= current.len() {
                break;
            }
            n = (n * 2).min(current.len());
        } else {
            n = 2;
        }
    }
    current.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn minimizes_to_single_failing_line() {
        let src = "line1\nline2 BAD\nline3\nline4";
        let deadline = Instant::now() + Duration::from_secs(5);
        let min = minimize(src, |s| s.contains("BAD"), deadline);
        assert_eq!(min.trim(), "line2 BAD");
    }

    #[test]
    fn returns_original_when_predicate_false_on_original() {
        let src = "no token here";
        let deadline = Instant::now() + Duration::from_secs(1);
        let min = minimize(src, |s| s.contains("XYZ"), deadline);
        assert_eq!(min, src);
    }

    #[test]
    fn time_boxed() {
        let src = (0..1000)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let deadline = Instant::now() + Duration::from_millis(50);
        let min = minimize(&src, |s| s.contains("line 999"), deadline);
        // Whatever shrink we achieved still contains the witness:
        assert!(min.contains("line 999"));
    }
}
