//! `specere harness tui` — ratatui companion for the harness inspector
//! (FR-HM-070..072).
//!
//! Architectural split:
//!
//! - [`TuiState`] — pure data + [`TuiState::handle_key`] state machine.
//!   No I/O, no rendering; unit-testable with zero setup.
//! - [`render`] — pure layout: takes a ratatui `Frame` + state, paints
//!   widgets. Unit-testable with ratatui's `TestBackend` snapshot buffer.
//! - [`run`] — the interactive loop that wires `crossterm` events to
//!   `handle_key` and `render` through a fullscreen terminal. Integration-
//!   testable via `--headless-frames` which paints one frame and exits
//!   (so CI can at least confirm the widget tree builds end-to-end).
//!
//! Keybindings (FR-HM-070):
//!
//! | Key        | Action                                     |
//! |------------|--------------------------------------------|
//! | `j` / ↓    | Select next harness node                   |
//! | `k` / ↑    | Select previous                            |
//! | `Enter`    | Expand relation-inspector mini-view        |
//! | `Esc`      | Close inspector / return to main pane      |
//! | `Tab`      | Cycle focus: Nodes → Clusters → Events    |
//! | `/`        | Filter nodes by substring                  |
//! | `q` / `Q`  | Quit                                        |

use std::collections::BTreeMap;

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::harness::node::HarnessGraph;

/// Which pane has focus right now (drives key routing).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Nodes,
    Clusters,
    Events,
}

/// Whether the relation-inspector overlay is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overlay {
    None,
    Inspector,
}

/// One key-driven transition the state machine can emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Redraw (nothing changed, but selection-dependent view wants refresh).
    Redraw,
    /// Selection moved / filter changed / overlay opened-closed.
    Changed,
    /// User wants to exit the TUI.
    Quit,
}

/// Full TUI state — intentionally serialisable-ish so snapshot tests are easy.
#[derive(Debug, Clone)]
pub struct TuiState {
    pub graph: HarnessGraph,
    pub focus: Focus,
    pub overlay: Overlay,
    /// Filter substring — empty = show all.
    pub filter: String,
    /// Index into `visible_nodes()`.
    pub node_idx: usize,
    /// Recent events for the timeline pane (latest 20 or so).
    pub events_snapshot: Vec<String>,
}

impl TuiState {
    pub fn new(graph: HarnessGraph, events: Vec<String>) -> Self {
        Self {
            graph,
            focus: Focus::Nodes,
            overlay: Overlay::None,
            filter: String::new(),
            node_idx: 0,
            events_snapshot: events,
        }
    }

    /// Handle one key event; return an [`Action`] describing what
    /// changed. Pure — no I/O.
    pub fn handle_key(&mut self, key: KeyCode) -> Action {
        if self.overlay == Overlay::Inspector {
            match key {
                KeyCode::Esc => {
                    self.overlay = Overlay::None;
                    return Action::Changed;
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => return Action::Quit,
                _ => return Action::Redraw,
            }
        }
        match key {
            KeyCode::Char('q') | KeyCode::Char('Q') => Action::Quit,
            KeyCode::Char('j') | KeyCode::Down => {
                let n = self.visible_nodes().len();
                if n > 0 {
                    self.node_idx = (self.node_idx + 1).min(n - 1);
                }
                Action::Changed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.node_idx = self.node_idx.saturating_sub(1);
                Action::Changed
            }
            KeyCode::Enter => {
                self.overlay = Overlay::Inspector;
                Action::Changed
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Nodes => Focus::Clusters,
                    Focus::Clusters => Focus::Events,
                    Focus::Events => Focus::Nodes,
                };
                Action::Changed
            }
            KeyCode::Char('/') => {
                // Filter-start — caller appends chars on next events.
                self.filter.clear();
                Action::Changed
            }
            KeyCode::Backspace => {
                self.filter.pop();
                self.node_idx = 0;
                Action::Changed
            }
            KeyCode::Char(c) if self.filter_active() => {
                self.filter.push(c);
                self.node_idx = 0;
                Action::Changed
            }
            _ => Action::Redraw,
        }
    }

    fn filter_active(&self) -> bool {
        !self.filter.is_empty()
    }

    /// Node paths currently shown in the left pane after filter.
    pub fn visible_nodes(&self) -> Vec<&crate::harness::node::HarnessFile> {
        self.graph
            .nodes
            .iter()
            .filter(|n| self.filter.is_empty() || n.path.contains(&self.filter))
            .collect()
    }

    pub fn selected_node(&self) -> Option<&crate::harness::node::HarnessFile> {
        self.visible_nodes().get(self.node_idx).copied()
    }

    /// Count of per-category nodes — the right-pane summary.
    pub fn category_counts(&self) -> BTreeMap<&'static str, usize> {
        let mut m = BTreeMap::new();
        for n in &self.graph.nodes {
            *m.entry(n.category.as_str()).or_insert(0) += 1;
        }
        m
    }
}

/// Mini key-code enum decoupled from crossterm — so unit tests don't
/// need a terminal open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Char(char),
    Enter,
    Esc,
    Up,
    Down,
    Tab,
    Backspace,
}

/// Map a crossterm KeyCode to our pure KeyCode.
pub fn from_crossterm(k: crossterm::event::KeyCode) -> Option<KeyCode> {
    use crossterm::event::KeyCode as C;
    Some(match k {
        C::Char(c) => KeyCode::Char(c),
        C::Enter => KeyCode::Enter,
        C::Esc => KeyCode::Esc,
        C::Up => KeyCode::Up,
        C::Down => KeyCode::Down,
        C::Tab => KeyCode::Tab,
        C::Backspace => KeyCode::Backspace,
        _ => return None,
    })
}

/// Pure render function — takes a Frame + state, paints the widget tree.
/// Used by both [`run`] and the unit tests' TestBackend-driven snapshot.
pub fn render(f: &mut Frame, state: &TuiState) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Header
            Constraint::Min(10),   // Body
            Constraint::Length(3), // Event timeline footer
        ])
        .split(area);

    render_header(f, chunks[0], state);
    render_body(f, chunks[1], state);
    render_events(f, chunks[2], state);

    if state.overlay == Overlay::Inspector {
        render_inspector(f, area, state);
    }
}

fn render_header(f: &mut Frame, area: Rect, state: &TuiState) {
    let n_files = state.graph.nodes.len();
    let n_clusters = state
        .graph
        .cluster_report
        .as_ref()
        .map(|r| r.n_clusters)
        .unwrap_or(0);
    let title = format!(
        " SpecERE harness inspector — {} file(s), {} cluster(s)  [Tab: focus  q: quit] ",
        n_files, n_clusters
    );
    let style = Style::default()
        .fg(Color::Black)
        .bg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);
    let p = Paragraph::new(title).style(style);
    f.render_widget(p, area);
}

fn render_body(f: &mut Frame, area: Rect, state: &TuiState) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    render_nodes_pane(f, cols[0], state);
    render_detail_pane(f, cols[1], state);
}

fn render_nodes_pane(f: &mut Frame, area: Rect, state: &TuiState) {
    let focused = matches!(state.focus, Focus::Nodes);
    let title = if state.filter.is_empty() {
        " Nodes ".to_string()
    } else {
        format!(" Nodes  [/{}] ", state.filter)
    };
    let items: Vec<ListItem> = state
        .visible_nodes()
        .iter()
        .map(|n| {
            let cat = n.category.as_str();
            let cluster = n.cluster_id.as_deref().unwrap_or("—");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("[{:<11}] ", cat),
                    Style::default().fg(cat_color(cat)),
                ),
                Span::raw(n.path.clone()),
                Span::styled(
                    format!("  ({cluster})"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style(focused)),
        )
        .highlight_style(Style::default().bg(Color::Blue).fg(Color::White))
        .highlight_symbol("▶ ");
    let mut s = ListState::default();
    s.select(Some(state.node_idx));
    f.render_stateful_widget(list, area, &mut s);
}

fn render_detail_pane(f: &mut Frame, area: Rect, state: &TuiState) {
    let focused = matches!(state.focus, Focus::Clusters);
    let mut lines: Vec<Line> = Vec::new();
    if let Some(node) = state.selected_node() {
        lines.push(Line::from(vec![
            Span::styled("path: ", Style::default().fg(Color::Gray)),
            Span::raw(node.path.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("id: ", Style::default().fg(Color::Gray)),
            Span::raw(node.id.clone()),
        ]));
        lines.push(Line::from(vec![
            Span::styled("category: ", Style::default().fg(Color::Gray)),
            Span::styled(
                node.category.as_str(),
                Style::default().fg(cat_color(node.category.as_str())),
            ),
        ]));
        if let Some(cr) = &node.crate_name {
            lines.push(Line::from(format!("crate: {cr}")));
        }
        if !node.test_names.is_empty() {
            lines.push(Line::from(format!(
                "test_names: {} entries",
                node.test_names.len()
            )));
        }
        if let Some(vm) = &node.version_metrics {
            lines.push(Line::from(format!(
                "commits: {}  churn: {:.2}  age: {}d  bus_factor: {}",
                vm.commits, vm.churn_rate, vm.age_days, vm.bus_factor
            )));
            lines.push(Line::from(format!(
                "hotspot_score: {:.2}",
                vm.hotspot_score
            )));
        }
        if let Some(h) = &node.coverage_hash {
            lines.push(Line::from(format!("coverage_hash: {h}")));
        }
        if let Some(fs) = node.flakiness_score {
            lines.push(Line::from(format!("flakiness_score: {fs:.4}")));
        }
        if let Some(cid) = &node.cluster_id {
            lines.push(Line::from(format!("cluster_id: {cid}")));
        }
        if let Some(p) = &node.provenance {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "── provenance ──",
                Style::default().add_modifier(Modifier::BOLD),
            )));
            if let Some(v) = &p.creator_verb {
                lines.push(Line::from(format!("creator_verb: /speckit-{v}")));
            }
            if let Some(a) = &p.creator_agent {
                lines.push(Line::from(format!("creator_agent: {a}")));
            }
            if let Some(c) = &p.creator_commit {
                lines.push(Line::from(format!(
                    "creator_commit: {}",
                    &c[..c.len().min(10)]
                )));
            }
            if let Some(h) = &p.creator_human {
                lines.push(Line::from(format!("creator_human: {h}")));
            }
            if p.divergence_detected {
                lines.push(Line::from(Span::styled(
                    "⚠ divergence_detected",
                    Style::default().fg(Color::Yellow),
                )));
            }
        }
    } else {
        // Summary fallback.
        lines.push(Line::from(Span::styled(
            "per-category counts",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for (cat, n) in state.category_counts() {
            lines.push(Line::from(format!("  {cat:<12} {n}")));
        }
    }

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Detail  [Enter to open inspector] ")
                .borders(Borders::ALL)
                .border_style(border_style(focused)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_events(f: &mut Frame, area: Rect, state: &TuiState) {
    let focused = matches!(state.focus, Focus::Events);
    let body: Vec<Line> = state
        .events_snapshot
        .iter()
        .rev()
        .take(3)
        .map(|e| Line::from(e.clone()))
        .collect();
    let p = Paragraph::new(body)
        .block(
            Block::default()
                .title(" Recent events ")
                .borders(Borders::ALL)
                .border_style(border_style(focused)),
        )
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn render_inspector(f: &mut Frame, area: Rect, state: &TuiState) {
    // Center a ~70% × 60% rect over the base area.
    let w = area.width * 70 / 100;
    let h = area.height * 60 / 100;
    let x = area.x + (area.width - w) / 2;
    let y = area.y + (area.height - h) / 2;
    let r = Rect::new(x, y, w, h);
    // Clear the background by painting an empty block.
    f.render_widget(Block::default().borders(Borders::NONE), r);

    let mut lines: Vec<Line> = Vec::new();
    if let Some(node) = state.selected_node() {
        lines.push(Line::from(Span::styled(
            "Relation inspector — Esc to close",
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(Line::from(format!("file: {}", node.path)));

        // Incoming direct_use: edges where to_path == node.path.
        let incoming: Vec<&str> = state
            .graph
            .edges
            .iter()
            .filter(|e| e.to == node.id)
            .map(|e| e.from_path.as_str())
            .collect();
        let outgoing: Vec<&str> = state
            .graph
            .edges
            .iter()
            .filter(|e| e.from == node.id)
            .map(|e| e.to_path.as_str())
            .collect();
        lines.push(Line::from(format!(
            "incoming (direct_use): {}",
            incoming.len()
        )));
        for p in incoming.iter().take(6) {
            lines.push(Line::from(format!("  ← {p}")));
        }
        lines.push(Line::from(format!(
            "outgoing (direct_use): {}",
            outgoing.len()
        )));
        for p in outgoing.iter().take(6) {
            lines.push(Line::from(format!("  → {p}")));
        }
        // cov_cooccur / cofail / comod neighbours.
        let cov = state
            .graph
            .cov_cooccur_edges
            .iter()
            .filter(|e| e.from == node.id || e.to == node.id)
            .count();
        let cofail = state
            .graph
            .cofail_edges
            .iter()
            .filter(|e| e.from == node.id || e.to == node.id)
            .count();
        let comod = state
            .graph
            .comod_edges
            .iter()
            .filter(|e| e.from == node.id || e.to == node.id)
            .count();
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "cov_cooccur: {cov}   cofail: {cofail}   comod: {comod}"
        )));
    } else {
        lines.push(Line::from("no node selected"));
    }

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Inspector ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .style(Style::default().bg(Color::Black))
        .wrap(Wrap { trim: false });
    f.render_widget(p, r);
}

fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn cat_color(cat: &str) -> Color {
    match cat {
        "unit" => Color::Green,
        "integration" => Color::LightGreen,
        "property" => Color::Magenta,
        "fuzz" => Color::Red,
        "bench" => Color::Yellow,
        "snapshot" => Color::Cyan,
        "fixture" => Color::DarkGray,
        "mock" => Color::LightMagenta,
        "workflow" => Color::Blue,
        "golden" => Color::LightYellow,
        _ => Color::White,
    }
}

/// Interactive loop — enters fullscreen, processes key events until
/// `q` or Ctrl-C. `headless_frames > 0` renders N frames then exits
/// without reading keys; used by integration tests to smoke-test the
/// widget tree end-to-end.
pub fn run(state: TuiState, headless_frames: u32) -> anyhow::Result<()> {
    use crossterm::{
        event::{self, Event, KeyEventKind},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io::stdout;

    let mut state = state;

    if headless_frames > 0 {
        // Test path: render to a test backend, don't touch the real TTY.
        use ratatui::backend::TestBackend;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend)?;
        for _ in 0..headless_frames {
            terminal.draw(|f| render(f, &state))?;
        }
        return Ok(());
    }

    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout());
    let mut terminal = Terminal::new(backend)?;
    let result: anyhow::Result<()> = (|| {
        loop {
            terminal.draw(|f| render(f, &state))?;
            if let Event::Key(k) = event::read()? {
                if k.kind != KeyEventKind::Press {
                    continue;
                }
                if let Some(code) = from_crossterm(k.code) {
                    match state.handle_key(code) {
                        Action::Quit => break,
                        _ => continue,
                    }
                }
            }
        }
        Ok(())
    })();
    // Always tear down the terminal, even on error.
    disable_raw_mode().ok();
    execute!(stdout(), LeaveAlternateScreen).ok();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::node::{path_id, Category, HarnessFile};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn sample_graph() -> HarnessGraph {
        HarnessGraph {
            schema_version: 1,
            nodes: vec![
                HarnessFile {
                    id: path_id("tests/a.rs"),
                    path: "tests/a.rs".into(),
                    category: Category::Integration,
                    category_confidence: 1.0,
                    crate_name: None,
                    test_names: vec!["test_a".into()],
                    provenance: None,
                    version_metrics: None,
                    coverage_hash: None,
                    flakiness_score: None,
                    cluster_id: Some("C01".into()),
                },
                HarnessFile {
                    id: path_id("tests/b.rs"),
                    path: "tests/b.rs".into(),
                    category: Category::Unit,
                    category_confidence: 0.9,
                    crate_name: None,
                    test_names: vec![],
                    provenance: None,
                    version_metrics: None,
                    coverage_hash: None,
                    flakiness_score: None,
                    cluster_id: Some("C01".into()),
                },
            ],
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
            cluster_report: None,
        }
    }

    #[test]
    fn j_moves_selection_down() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        assert_eq!(s.node_idx, 0);
        let a = s.handle_key(KeyCode::Char('j'));
        assert_eq!(a, Action::Changed);
        assert_eq!(s.node_idx, 1);
    }

    #[test]
    fn k_moves_selection_up_but_never_below_zero() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        s.handle_key(KeyCode::Char('k'));
        assert_eq!(s.node_idx, 0);
    }

    #[test]
    fn j_stays_clamped_at_end() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        for _ in 0..10 {
            s.handle_key(KeyCode::Char('j'));
        }
        assert_eq!(s.node_idx, 1, "should clamp at last node");
    }

    #[test]
    fn tab_cycles_focus() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        assert_eq!(s.focus, Focus::Nodes);
        s.handle_key(KeyCode::Tab);
        assert_eq!(s.focus, Focus::Clusters);
        s.handle_key(KeyCode::Tab);
        assert_eq!(s.focus, Focus::Events);
        s.handle_key(KeyCode::Tab);
        assert_eq!(s.focus, Focus::Nodes);
    }

    #[test]
    fn enter_opens_inspector_esc_closes_it() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        s.handle_key(KeyCode::Enter);
        assert_eq!(s.overlay, Overlay::Inspector);
        s.handle_key(KeyCode::Esc);
        assert_eq!(s.overlay, Overlay::None);
    }

    #[test]
    fn q_quits() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        assert_eq!(s.handle_key(KeyCode::Char('q')), Action::Quit);
    }

    #[test]
    fn filter_narrows_visible_nodes() {
        let mut s = TuiState::new(sample_graph(), vec![]);
        s.filter = "a.rs".to_string();
        let v = s.visible_nodes();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].path, "tests/a.rs");
    }

    #[test]
    fn render_runs_without_panic_on_headless_backend() {
        let state = TuiState::new(sample_graph(), vec!["sample event".to_string()]);
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &state)).unwrap();
        // Grab the buffer and assert the header string is somewhere in it.
        let buf = terminal.backend().buffer();
        let mut joined = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("SpecERE harness inspector"),
            "header must be present in rendered buffer"
        );
    }

    #[test]
    fn render_inspector_overlay_shows_title() {
        let mut state = TuiState::new(sample_graph(), vec![]);
        state.overlay = Overlay::Inspector;
        let backend = TestBackend::new(120, 30);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| render(f, &state)).unwrap();
        let buf = terminal.backend().buffer();
        let mut joined = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                joined.push_str(buf[(x, y)].symbol());
            }
            joined.push('\n');
        }
        assert!(
            joined.contains("Relation inspector"),
            "inspector overlay must render its title"
        );
    }

    #[test]
    fn category_counts_matches_graph_nodes() {
        let state = TuiState::new(sample_graph(), vec![]);
        let c = state.category_counts();
        assert_eq!(c.get("integration"), Some(&1));
        assert_eq!(c.get("unit"), Some(&1));
    }

    #[test]
    fn empty_graph_doesnt_panic_on_keypress() {
        let empty = HarnessGraph {
            schema_version: 1,
            nodes: Vec::new(),
            edges: Vec::new(),
            comod_edges: Vec::new(),
            cov_cooccur_edges: Vec::new(),
            cofail_edges: Vec::new(),
            cluster_report: None,
        };
        let mut s = TuiState::new(empty, vec![]);
        s.handle_key(KeyCode::Char('j'));
        s.handle_key(KeyCode::Char('k'));
        s.handle_key(KeyCode::Enter);
        // No selected node but no crash.
        assert!(s.selected_node().is_none());
    }
}
