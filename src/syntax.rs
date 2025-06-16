use std::collections::HashMap;
use rowan::{GreenNodeBuilder, SyntaxKind};
use crate::ast::ConTeXtNode;
use rowan::Language;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u16)]
pub enum TexSyntaxKind {
    // Document structure
    Document,
    Preamble,
    Body,
    
    // Node types
    Command,
    StartStop,
    Text,
    Comment,
    OptionGroup,
    
    // Token subtypes
    CommandName,
    OptionKey,
    OptionValue,
    Punctuation,
    EnvName,
}

// 2. Implement Rowan Language trait
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

// 3. From/Into implementations
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

// 4. Type aliases for convenience
pub type SyntaxNode = rowan::SyntaxNode<ConTeXtLanguage>;
pub type SyntaxToken = rowan::SyntaxToken<ConTeXtLanguage>;
pub type GreenNode = rowan::GreenNode;

// 5. Core conversion function
pub fn ast_to_rowan(root_node: ConTeXtNode) -> GreenNode {
    let mut builder = GreenNodeBuilder::new();
    
    match root_node {
        ConTeXtNode::Document { preamble, body } => {
            builder.start_node(TexSyntaxKind::Document.into());
            
            // Preamble section
            builder.start_node(TexSyntaxKind::Preamble.into());
            for node in preamble {
                add_node(&mut builder, &node);
            }
            builder.finish_node();
            
            // Body section
            builder.start_node(TexSyntaxKind::Body.into());
            for node in body {
                add_node(&mut builder, &node);
            }
            builder.finish_node();
            
            builder.finish_node()
        }
        other_node => {
            // Handle case where root isn't a Document (shouldn't happen in normal usage)
            builder.start_node(TexSyntaxKind::Document.into());
            add_node(&mut builder, &other_node);
            builder.finish_node()
        }
    }
    
    builder.finish()
}

// 6. Helper for converting individual nodes
fn add_node(builder: &mut GreenNodeBuilder, node: &ConTeXtNode) {
    match node {
        ConTeXtNode::Command { name, options, arguments, .. } => {
            builder.start_node(TexSyntaxKind::Command.into());
            
            // Command name
            builder.token(TexSyntaxKind::CommandName.into(), name.as_str());
            
            // Options
            if !options.is_empty() {
                builder.start_node(TexSyntaxKind::OptionGroup.into());
                for (i, (k, v)) in options.iter().enumerate() {
                    if i > 0 {
                        builder.token(TexSyntaxKind::Punctuation.into(), ",");
                    }
                    builder.token(TexSyntaxKind::OptionKey.into(), k.as_str());
                    builder.token(TexSyntaxKind::Punctuation.into(), "=");
                    builder.token(TexSyntaxKind::OptionValue.into(), v.as_str());
                }
                builder.finish_node();
            }
            
            // Arguments
            for arg in arguments {
                add_node(builder, arg);
            }
            
            builder.finish_node();
        },
        
        ConTeXtNode::StartStop { name, content, .. } => {
            builder.start_node(TexSyntaxKind::StartStop.into());
            builder.token(TexSyntaxKind::EnvName.into(), name.as_str());
            
            for node in content {
                add_node(builder, node);
            }
            
            builder.finish_node();
        },
        
        ConTeXtNode::Text { content, .. } => {
            builder.token(TexSyntaxKind::Text.into(), content.as_str());
        },
        
        ConTeXtNode::Comment { content, .. } => {
            builder.token(TexSyntaxKind::Comment.into(), content.as_str());
        },
        
        // Handle Document variant if nested (unlikely but possible)
        ConTeXtNode::Document { .. } => {
            let node = node.clone();
            add_node(builder, &node);
        }
    }
}

// use rowan::{GreenNodeBuilder, GreenNode, SyntaxKind};
// 
// use crate::ast::ConTeXtNode;
// 
// use rowan::Language;
// 
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
// pub enum ConTeXtLanguage {}
// 
// impl Language for ConTeXtLanguage {
//     type Kind = TexSyntaxKind;
//     
//     fn kind_from_raw(raw: rowan::SyntaxKind) -> Self::Kind {
//         TexSyntaxKind::from(raw)
//     }
//     
//     fn kind_to_raw(kind: Self::Kind) -> rowan::SyntaxKind {
//         kind.into()
//     }
// }
// 
// pub type SyntaxNode = rowan::SyntaxNode<ConTeXtLanguage>;
// pub type SyntaxToken = rowan::SyntaxToken<ConTeXtLanguage>;
// pub type SyntaxElement = rowan::SyntaxElement<ConTeXtLanguage>;
// 
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
// #[repr(u16)]
// pub enum TexSyntaxKind {
//     Document,
//     Error,
//     
//     Command,
//     StartStop,
//     Text,
//     Comment,
//     
//     CommandName,
//     OptionGroup,
//     OptionKey,
//     OptionValue,
//     EnvName,
//     Punctuation,
// 
//     Preamble,
//     Body,
// }
// 
// impl From<TexSyntaxKind> for SyntaxKind {
//     fn from(kind: TexSyntaxKind) -> Self {
//         SyntaxKind(kind as u16)
//     }
// }
// 
// impl From<SyntaxKind> for TexSyntaxKind {
//     fn from(kind: SyntaxKind) -> Self {
//         unsafe { std::mem::transmute(kind.0) }
//     }
// }
// 
// pub fn ast_to_rowan(ast: Vec<ConTeXtNode>) -> rowan::GreenNode {
//     let mut builder = rowan::GreenNodeBuilder::new();
//     
//     builder.start_node(TexSyntaxKind::Document.into());
//     
//     for node in ast {
//         match node {
//             ConTeXtNode::Command { name, options, arguments, .. } => {
//                 builder.start_node(TexSyntaxKind::Command.into());
//                 
//                 // Command name
//                 builder.token(TexSyntaxKind::CommandName.into(), name.as_str());
//                 
//                 // Options
//                 if !options.is_empty() {
//                     builder.start_node(TexSyntaxKind::OptionGroup.into());
//                     for (i, (k, v)) in options.iter().enumerate() {
//                         if i > 0 {
//                             builder.token(TexSyntaxKind::Punctuation.into(), ",");
//                         }
//                         builder.token(TexSyntaxKind::OptionKey.into(), k.as_str());
//                         builder.token(TexSyntaxKind::Punctuation.into(), "=");
//                         builder.token(TexSyntaxKind::OptionValue.into(), v.as_str());
//                     }
//                     builder.finish_node();
//                 }
//                 
//                 // Arguments
//                 for arg in arguments {
//                     add_node(&mut builder, &arg);
//                 }
//                 
//                 builder.finish_node();
//             },
//             // ... other variants
//         }
//     }
//     
//     builder.finish_node(); // Closes the Document node
//     builder.finish()       // Returns the GreenNode
// }
// 
// #[inline]
// fn add_node(builder: &mut GreenNodeBuilder, node: &ConTeXtNode) {
//     ast_to_rowan(vec![node.clone()]);
// }
