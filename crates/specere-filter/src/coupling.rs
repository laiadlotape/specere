//! Coupling graph loader for `.specere/sensor-map.toml`.
//!
//! Format (under the `[coupling]` table):
//!
//! ```toml
//! [coupling]
//! edges = [
//!   ["FR-001", "FR-002"],
//!   ["FR-002", "FR-003"],
//! ]
//! ```
//!
//! Edges are **directed** — `["A", "B"]` propagates a pair-factor from `A`
//! into `B`'s belief on each BP sweep. FR-P4-006: the loader rejects cycles
//! with an actionable error naming the offending chain. RBPF (#42) is the
//! escape valve for anyone whose coupling model *needs* cycles; for BP, we
//! require a DAG.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;

/// Directed edge `(src, dst)`.
pub type Edge = (String, String);

#[derive(Debug, Clone, Default)]
pub struct CouplingGraph {
    pub edges: Vec<Edge>,
}

impl CouplingGraph {
    /// Parse `[coupling].edges` from a TOML file. Missing file or missing
    /// `[coupling]` section both yield an empty graph — that's the
    /// "no cross-spec coupling" case, equivalent to plain `PerSpecHMM`.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw =
            std::fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        Self::from_toml_str(&raw)
    }

    pub fn from_toml_str(raw: &str) -> Result<Self> {
        #[derive(Deserialize)]
        struct Root {
            coupling: Option<CouplingSec>,
        }
        #[derive(Deserialize)]
        struct CouplingSec {
            edges: Option<Vec<Vec<String>>>,
        }
        let parsed: Root = toml::from_str(raw).context("parse sensor-map.toml")?;
        let raw_edges = parsed.coupling.and_then(|c| c.edges).unwrap_or_default();
        let mut edges: Vec<Edge> = Vec::with_capacity(raw_edges.len());
        for pair in raw_edges {
            if pair.len() != 2 {
                return Err(anyhow!(
                    "coupling.edges entries must be length-2 arrays like [\"A\",\"B\"]; found arity {}",
                    pair.len()
                ));
            }
            edges.push((pair[0].clone(), pair[1].clone()));
        }
        let g = Self { edges };
        g.require_dag()?;
        Ok(g)
    }

    /// FR-P4-006: reject cycles with an actionable chain. DFS with a
    /// three-colour state — cycle found iff we traverse back into a node
    /// currently on the recursion stack. Error names the cycle chain so
    /// users can fix their sensor-map rather than grep in the dark.
    pub fn require_dag(&self) -> Result<()> {
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut nodes: Vec<&str> = Vec::new();
        for (src, dst) in &self.edges {
            adj.entry(src.as_str()).or_default().push(dst.as_str());
            nodes.push(src.as_str());
            nodes.push(dst.as_str());
        }
        nodes.sort();
        nodes.dedup();

        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Colour {
            White,
            Gray,
            Black,
        }
        let mut colour: HashMap<&str, Colour> = nodes.iter().map(|n| (*n, Colour::White)).collect();

        for start in &nodes {
            if colour[start] != Colour::White {
                continue;
            }
            // Iterative DFS with a stack of (node, child-iter-index). `path`
            // is the current recursion chain — we reach back into it when a
            // cycle closes.
            let mut stack: Vec<(&str, usize)> = vec![(start, 0)];
            let mut path: Vec<&str> = vec![start];
            colour.insert(start, Colour::Gray);
            while let Some(&mut (node, ref mut child_i)) = stack.last_mut() {
                let children = adj.get(node).map(|v| v.as_slice()).unwrap_or(&[]);
                if *child_i < children.len() {
                    let next = children[*child_i];
                    *child_i += 1;
                    match colour.get(next).copied().unwrap_or(Colour::White) {
                        Colour::White => {
                            colour.insert(next, Colour::Gray);
                            path.push(next);
                            stack.push((next, 0));
                        }
                        Colour::Gray => {
                            // Cycle closed. Extract the chain from the first
                            // occurrence of `next` in path through to the end.
                            let start_i = path.iter().position(|&p| p == next).unwrap();
                            let mut chain: Vec<&str> = path[start_i..].to_vec();
                            chain.push(next);
                            return Err(anyhow!(
                                "coupling graph has a cycle ({}); BP requires a DAG — \
                                 remove one of these edges or route the cluster through RBPF (#42)",
                                chain.join(" -> ")
                            ));
                        }
                        Colour::Black => {}
                    }
                } else {
                    colour.insert(node, Colour::Black);
                    path.pop();
                    stack.pop();
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_empty_graph() {
        let g = CouplingGraph::from_toml_str("").unwrap();
        assert!(g.edges.is_empty());
    }

    #[test]
    fn accepts_tree() {
        let g = CouplingGraph::from_toml_str(
            r#"
            [coupling]
            edges = [
              ["A", "B"],
              ["A", "C"],
              ["B", "D"],
            ]
            "#,
        )
        .unwrap();
        assert_eq!(g.edges.len(), 3);
    }

    #[test]
    fn rejects_self_loop() {
        let err = CouplingGraph::from_toml_str(
            r#"
            [coupling]
            edges = [["A", "A"]]
            "#,
        )
        .unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cycle") && msg.contains("A -> A"),
            "unexpected error message: {msg}"
        );
    }

    #[test]
    fn rejects_triangle() {
        let err = CouplingGraph::from_toml_str(
            r#"
            [coupling]
            edges = [["A","B"], ["B","C"], ["C","A"]]
            "#,
        )
        .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("cycle"), "expected cycle error, got: {msg}");
        assert!(
            msg.contains("A") && msg.contains("B") && msg.contains("C"),
            "chain did not name all nodes: {msg}"
        );
    }

    #[test]
    fn rejects_malformed_edge() {
        let err = CouplingGraph::from_toml_str(
            r#"
            [coupling]
            edges = [["A", "B", "C"]]
            "#,
        )
        .unwrap_err();
        assert!(format!("{err}").contains("arity"));
    }

    #[test]
    fn missing_file_is_empty_graph() {
        let g = CouplingGraph::load(std::path::Path::new("/nonexistent/path.toml")).unwrap();
        assert!(g.edges.is_empty());
    }
}
