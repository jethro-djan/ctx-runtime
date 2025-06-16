use crate::syntax::TexSyntaxKind;
use rowan::{SyntaxNode, NodeOrToken};
use std::ops::Range;

use crate::syntax;

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

pub fn run(node: &syntax::SyntaxNode) -> Vec<Highlight> {
    let mut highlights = Vec::new();
    let mut stack = Vec::new();
    
    // Depth-first traversal
    for event in node.preorder_with_tokens() {
        match event {
            rowan::WalkEvent::Enter(node_or_token) => {
                match node_or_token {
                    NodeOrToken::Node(n) => {
                        stack.push(n.kind());
                    }
                    NodeOrToken::Token(t) => {
                        let kind = match t.kind() {
                            TexSyntaxKind::CommandName => HighlightKind::Command,
                            TexSyntaxKind::EnvName => HighlightKind::Environment,
                            TexSyntaxKind::OptionGroup => HighlightKind::Option,
                            TexSyntaxKind::Comment => HighlightKind::Comment,
                            TexSyntaxKind::Text => HighlightKind::Text,
                            _ => stack.last().copied()
                                .and_then(|k| match k {
                                    TexSyntaxKind::Command => Some(HighlightKind::Command),
                                    TexSyntaxKind::StartStop => Some(HighlightKind::Keyword),
                                    _ => None
                                })
                                .unwrap_or(HighlightKind::Text),
                        };
                        
                        let range = t.text_range();
                        highlights.push(Highlight {
                            range: range.start().into()..range.end().into(),
                            kind,
                        });
                    }
                }
            }
            rowan::WalkEvent::Leave(node_or_token) => {
                if let NodeOrToken::Node(_) = node_or_token {
                    stack.pop();
                }
            }
        }
    }
    
    highlights
}
