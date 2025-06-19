use rowan::{GreenNodeBuilder, SyntaxKind};
use crate::ast::ConTeXtNode;
use rowan::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum TexSyntaxKind {
    // Document structure
    DOCUMENT,
    PREAMBLE,
    BODY,
    
    // Node types
    COMMAND,
    STARTSTOP,
    TEXT,
    COMMENT,
    OPTIONGROUP,
    
    // Token subtypes
    COMMANDNAME,
    OPTIONKEY,
    OPTIONVALUE,
    PUNCTUATION,
    ENVNAME,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ConTeXtLanguage {}

impl Language for ConTeXtLanguage {
    type Kind = TexSyntaxKind;
    
    fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
        TexSyntaxKind::from(raw)
    }
    
    fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
        kind.into()
    }
}

impl From<TexSyntaxKind> for SyntaxKind {
    fn from(kind: TexSyntaxKind) -> Self {
        SyntaxKind(kind as u16)
    }
}

impl From<SyntaxKind> for TexSyntaxKind {
    fn from(kind: SyntaxKind) -> Self {
        unsafe { std::mem::transmute(kind.0) }
    }
}

pub type SyntaxNode = rowan::SyntaxNode<ConTeXtLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<ConTeXtLanguage>;
pub type GreenNode = rowan::GreenNode;

pub fn ast_to_rowan(root_node: ConTeXtNode) -> GreenNode {
    let mut builder = GreenNodeBuilder::new();
    
    match root_node {
        ConTeXtNode::Document { preamble, body } => {
            builder.start_node(TexSyntaxKind::DOCUMENT.into());
            
            builder.start_node(TexSyntaxKind::PREAMBLE.into());
            for node in preamble {
                add_node(&mut builder, &node);
            }
            builder.finish_node();
            
            builder.start_node(TexSyntaxKind::BODY.into());
            for node in body {
                add_node(&mut builder, &node);
            }
            builder.finish_node();
            
            builder.finish_node()
        }
        other_node => {

            builder.start_node(TexSyntaxKind::DOCUMENT.into());
            add_node(&mut builder, &other_node);
            builder.finish_node()
        }
    }
    
    builder.finish()
}

fn add_node(builder: &mut GreenNodeBuilder, node: &ConTeXtNode) {
    match node {
        ConTeXtNode::Command { name, options, arguments, .. } => {
            builder.start_node(TexSyntaxKind::COMMAND.into());
            
            builder.token(TexSyntaxKind::COMMANDNAME.into(), name.as_str());
            
            if !options.is_empty() {
                builder.start_node(TexSyntaxKind::OPTIONGROUP.into());
                for (i, (k, v)) in options.iter().enumerate() {
                    if i > 0 {
                        builder.token(TexSyntaxKind::PUNCTUATION.into(), ",");
                    }
                    builder.token(TexSyntaxKind::OPTIONKEY.into(), k.as_str());
                    builder.token(TexSyntaxKind::PUNCTUATION.into(), "=");
                    builder.token(TexSyntaxKind::OPTIONVALUE.into(), v.as_str());
                }
                builder.finish_node();
            }
            
            for arg in arguments {
                add_node(builder, arg);
            }
            
            builder.finish_node();
        },
        
        ConTeXtNode::StartStop { name, content, .. } => {
            builder.start_node(TexSyntaxKind::STARTSTOP.into());
            builder.token(TexSyntaxKind::ENVNAME.into(), name.as_str());
            
            for node in content {
                add_node(builder, node);
            }
            
            builder.finish_node();
        },
        
        ConTeXtNode::Text { content, .. } => {
            builder.token(TexSyntaxKind::TEXT.into(), content.as_str());
        },
        
        ConTeXtNode::Comment { content, .. } => {
            builder.token(TexSyntaxKind::COMMENT.into(), content.as_str());
        },
        
        ConTeXtNode::Document { .. } => {
            let node = node.clone();
            add_node(builder, &node);
        }
    }
}
