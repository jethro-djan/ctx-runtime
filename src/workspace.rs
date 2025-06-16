use crate::{parser, syntax::{self, SyntaxNode}, highlight::{Highlight, run}};
use crate::ast::ConTeXtNode;

pub struct Document {
    pub source: String,
    pub ast: ConTeXtNode,
    pub syntax_tree: SyntaxNode,
    pub highlights: Vec<Highlight>,
}

impl Document {
    #[uniffi::constructor]
    pub fn from_str(source: &str) -> Option<Self> {
        let ast = parser::parse_document(source).ok()?;
        let green = syntax::ast_to_rowan(ast.clone());
        let syntax_tree = SyntaxNode::new_root(green);
        let highlights = run(&syntax_tree);
        Some(Self {
            source: source.to_string(),
            ast,
            syntax_tree,
            highlights,
        })
    }
}

use std::collections::HashMap;

pub struct Workspace {
    documents: HashMap<String, Document>,
}

impl Workspace {
    pub fn new() -> Self {
        Self { documents: HashMap::new() }
    }

    pub fn open(&mut self, uri: &str, text: &str) -> bool {
        match Document::from_str(text) {
            Some(doc) => {
                self.documents.insert(uri.to_string(), doc);
                true
            }
            None => false,
        }
    }

    pub fn update(&mut self, uri: &str, text: &str) -> bool {
        self.open(uri, text)
    }

    pub fn highlights(&self, uri: &str) -> Option<&[Highlight]> {
        self.documents.get(uri).map(|d| d.highlights.as_slice())
    }

    pub fn ast(&self, uri: &str) -> Option<&ConTeXtNode> {
        self.documents.get(uri).map(|d| &d.ast)
    }

    pub fn source(&self, uri: &str) -> Option<&str> {
        self.documents.get(uri).map(|d| d.source.as_str())
    }
}
