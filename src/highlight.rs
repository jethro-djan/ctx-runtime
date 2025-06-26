use crate::syntax::{SyntaxKind, SyntaxNode};
use std::ops::Range;

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

pub fn highlight(node: &SyntaxNode) -> Vec<Highlight> {
    let mut highlights = Vec::new();
    highlight_node(node, &mut highlights);
    highlights
}

fn highlight_node(node: &SyntaxNode, highlights: &mut Vec<Highlight>) {
    match node.kind() {
        SyntaxKind::Command => {
            if let Some(token) = node.first_token() {
                highlights.push(Highlight {
                    range: text_range_to_std_range(token.text_range()),
                    kind: HighlightKind::Command,
                });
            }
        }
        SyntaxKind::Environment => {
            if let Some(token) = node.first_token() {
                highlights.push(Highlight {
                    range: text_range_to_std_range(token.text_range()),
                    kind: HighlightKind::Environment,
                });
            }
        }
        SyntaxKind::Options => {
            if let Some(token) = node.first_token() {
                highlights.push(Highlight {
                    range: text_range_to_std_range(token.text_range()),
                    kind: HighlightKind::Option,
                });
            }
        }
        SyntaxKind::Text => {
            if let Some(token) = node.first_token() {
                highlights.push(Highlight {
                    range: text_range_to_std_range(token.text_range()),
                    kind: HighlightKind::Text,
                });
            }
        }
        SyntaxKind::Comment => {
            if let Some(token) = node.first_token() {
                highlights.push(Highlight {
                    range: text_range_to_std_range(token.text_range()),
                    kind: HighlightKind::Comment,
                });
            }
        }
        _ => {}
    }
    
    for child in node.children() {
        highlight_node(&child, highlights);
    }
}

pub fn text_range_to_std_range(range: rowan::TextRange) -> Range<usize> {
    range.start().into()..range.end().into()
}

