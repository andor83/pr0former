//! Author-owned node documentation, delivered with the catalog to every client.
use crate::Graph;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeDocumentation {
    pub title: String,
    pub explanation: String,
    pub steps: Vec<String>,
    pub graph: Graph,
}

pub fn for_kind(kind: &str) -> Option<NodeDocumentation> {
    static DOCS: OnceLock<BTreeMap<String, NodeDocumentation>> = OnceLock::new();
    DOCS.get_or_init(|| {
        serde_json::from_str(include_str!("node_documentation.json"))
            .expect("bundled node documentation must be valid JSON")
    })
    .get(kind)
    .cloned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_catalog_example_is_complete_and_server_validated() {
        let catalog = crate::catalog();
        let authored: std::collections::BTreeMap<String, super::NodeDocumentation> =
            serde_json::from_str(include_str!("node_documentation.json")).unwrap();
        assert_eq!(
            authored.len(),
            catalog.len(),
            "remove stale documentation entries"
        );
        for descriptor in catalog {
            let doc = descriptor.documentation.expect(&descriptor.kind);
            assert!(
                !doc.title.is_empty() && !doc.explanation.is_empty(),
                "{}",
                descriptor.kind
            );
            assert!(
                doc.graph.nodes.iter().any(|n| n.kind == descriptor.kind),
                "{} must appear in its example",
                descriptor.kind
            );
            doc.graph
                .validate()
                .unwrap_or_else(|error| panic!("{} example: {error}", descriptor.kind));
        }
    }
}
