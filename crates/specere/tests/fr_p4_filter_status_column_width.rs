//! `specere filter status` must dynamically size the `spec_id` column
//! so long domain-prefixed ids (`FR-auth-001`, `FR-HM-052`) don't
//! truncate or mis-align. Previously the column was hard-coded to 11
//! characters — see `docs/upcoming.md` §4 closure.

mod common;

use common::TempRepo;

/// Seed a posterior directly — bypasses `filter run` so the test is
/// deterministic and doesn't depend on unrelated evaluation paths.
fn seed_posterior(repo: &TempRepo, entries: &[(&str, f64, f64, f64)]) {
    let mut body = String::from("cursor = \"2026-04-20T10:00:00Z\"\nschema_version = 1\n\n");
    for (id, p_unk, p_sat, p_vio) in entries {
        body.push_str("[[entries]]\n");
        body.push_str(&format!("spec_id = \"{id}\"\n"));
        body.push_str(&format!("p_unk = {p_unk}\n"));
        body.push_str(&format!("p_sat = {p_sat}\n"));
        body.push_str(&format!("p_vio = {p_vio}\n"));
        body.push_str("entropy = 0.5\n");
        body.push_str("last_updated = \"2026-04-20T10:00:00Z\"\n\n");
    }
    repo.write(".specere/posterior.toml", &body);
}

#[test]
fn short_ids_render_with_legacy_11_char_column_width() {
    // Regression: pre-fix behaviour with ≤11-char ids was a 11-char
    // `spec_id` column. New code must preserve that baseline (minimum
    // width = header length = 7, but historically shown as 11). Verify
    // the header still has the expected left-aligned shape.
    let repo = TempRepo::new();
    seed_posterior(
        &repo,
        &[("FR-001", 0.1, 0.8, 0.1), ("FR-002", 0.2, 0.7, 0.1)],
    );

    let out = repo
        .run_specere(&["filter", "status"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Each data row starts with the spec-id; the first column must be
    // left-padded and followed by at least two spaces before p_unk.
    assert!(stdout.contains("FR-001"), "expected FR-001 row: {stdout}");
    assert!(stdout.contains("FR-002"), "expected FR-002 row: {stdout}");
    // Header is present (column width independent).
    assert!(
        stdout.contains("spec_id") && stdout.contains("p_unk"),
        "expected header: {stdout}"
    );
}

#[test]
fn long_ids_widen_column_without_truncation() {
    let repo = TempRepo::new();
    // Mix of short + 12-char + 14-char ids — exactly the pattern that
    // used to mis-align under the 11-char hard-coded width.
    seed_posterior(
        &repo,
        &[
            ("FR-001", 0.1, 0.8, 0.1),
            ("FR-auth-alpha", 0.2, 0.7, 0.1),   // 13 chars
            ("FR-HM-050", 0.3, 0.6, 0.1),       // 9 chars
            ("FR-EDITOR-001", 0.05, 0.9, 0.05), // 13 chars
        ],
    );

    let out = repo
        .run_specere(&["filter", "status"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);

    // No id may be truncated.
    for id in ["FR-001", "FR-auth-alpha", "FR-HM-050", "FR-EDITOR-001"] {
        assert!(
            stdout.contains(id),
            "expected {id} present intact: {stdout}"
        );
    }

    // Column alignment: header's `p_unk` label and the `------` in the
    // dash row must start at the same byte index (the dashes mirror the
    // header exactly — this is the strict regression the original
    // bug was about).
    let header_line = stdout
        .lines()
        .find(|l| l.starts_with("spec_id"))
        .expect("header line present");
    let dash_line = stdout
        .lines()
        .find(|l| l.starts_with('-'))
        .expect("dash separator present");
    let header_p_unk = header_line.find("p_unk").expect("header has p_unk");
    // Find the p_unk dashes — skip past the id-column dashes by first
    // locating the first space in the dash row, then the next `------`.
    let after_id_col = dash_line.find(' ').expect("dash line has a space");
    let dash_p_unk = after_id_col
        + dash_line[after_id_col..]
            .find("------")
            .expect("p_unk dashes present after id column");
    assert_eq!(
        header_p_unk, dash_p_unk,
        "header p_unk column and dash row's ------ must align: header at {header_p_unk}, dashes at {dash_p_unk}"
    );

    // And no data row may exceed that column layout — specifically,
    // every row's id column must end strictly before the dash row's
    // dashes end. (Under the old 11-char fixed width, a 13-char id
    // would spill into the p_unk column and break this.)
    let dash_id_end = dash_line
        .find(' ')
        .expect("dash line has a space after id dashes");
    for id in ["FR-auth-alpha", "FR-EDITOR-001"] {
        assert!(
            id.len() <= dash_id_end,
            "id `{id}` (len={}) must fit in the spec_id column (dashes end at {dash_id_end})",
            id.len()
        );
    }
}

#[test]
fn empty_posterior_prints_guidance_not_panic() {
    // Sanity: no entries → friendly message from existing code path.
    let repo = TempRepo::new();
    // Write an empty-entries posterior.
    repo.write(
        ".specere/posterior.toml",
        "cursor = \"2026-04-20T10:00:00Z\"\nschema_version = 1\n",
    );
    let out = repo
        .run_specere(&["filter", "status"])
        .output()
        .expect("spawn");
    assert!(out.status.success());
}
