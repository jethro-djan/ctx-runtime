use std::ops::Range;
use crate::ast::ConTeXtNode;

#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    pub range: Range<usize>,
    pub kind: HighlightKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HighlightKind {
    Keyword,      
    Command,      
    Option,       
    Text,
    Comment,
    Environment,
}

impl HighlightKind {
    pub fn to_string(&self) -> String {
        match self {
            Self::Keyword => "keyword",
            Self::Command => "command",
            Self::Option => "option",
            Self::Text => "text",
            Self::Comment => "comment",
            Self::Environment => "environment",
        }.to_string()
    }
}

/// Main entry point for AST-based highlighting
pub fn highlight(node: &ConTeXtNode) -> Vec<Highlight> {
    let mut highlights = Vec::new();
    highlight_node(node, &mut highlights);
    highlights
}

/// Highlight AST nodes directly
fn highlight_node(node: &ConTeXtNode, highlights: &mut Vec<Highlight>) {
    match node {
        ConTeXtNode::Command { name, options, arguments, span, .. } => {
            highlights.push(Highlight {
                range: span.start..span.end,
                kind: HighlightKind::Command,
            });
            
            for arg in arguments {
                highlight_node(arg, highlights);
            }
        },
        
        ConTeXtNode::StartStop { content, span, .. } => {
            highlights.push(Highlight {
                range: span.start..span.end,
                kind: HighlightKind::Environment,
            });
            
            // Highlight content recursively
            for child in content {
                highlight_node(child, highlights);
            }
        },
        
        ConTeXtNode::Text { span, .. } => {
            highlights.push(Highlight {
                range: span.start..span.end,
                kind: HighlightKind::Text,
            });
        },
        
        ConTeXtNode::Comment { span, .. } => {
            highlights.push(Highlight {
                range: span.start..span.end,
                kind: HighlightKind::Comment,
            });
        },
        
        ConTeXtNode::Document { preamble, body } => {
            for node in preamble {
                highlight_node(node, highlights);
            }
            for node in body {
                highlight_node(node, highlights);
            }
        }
    }
}
