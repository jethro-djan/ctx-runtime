use crate::workspace::Workspace;
pub struct Runtime {
    workspace: Workspace,
}

impl Runtime {
    pub fn new() -> Self {
        Runtime {
            workspace: Workspace::new(),
        }
    }

    pub fn open(&mut self, uri: String, source: String) -> bool {
        self.workspace.open(&uri, &source)
    }

    pub fn get_highlights(&self, uri: String) -> Vec<HighlightFFI> {
        self.workspace.highlights(&uri)
            .unwrap_or_default()
            .iter()
            .map(|h| HighlightFFI {
                range: vec![h.range.start as u32, h.range.end as u32],
                kind: format!("{:?}", h.kind),
            })
            .collect()
    }
}

pub struct HighlightFFI {
    pub range: Vec<u32>,
    pub kind: String,
}
