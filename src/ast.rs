use std::collections::HashMap;
use nom_locate::LocatedSpan;
use serde::{Serialize, Deserialize};

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ConTeXtNode {
    Document {
        preamble: Vec<ConTeXtNode>,
        body: Vec<ConTeXtNode>,
    },
    Command {
        name: String,
        style: CommandStyle,
        arg_style: ArgumentStyle,
        options: HashMap<String, String>,
        arguments: Vec<ConTeXtNode>,
        span: SourceSpan,
    },
    StartStop {
        name: String,
        options: HashMap<String, String>,
        content: Vec<ConTeXtNode>,
        span: SourceSpan,
    },
    Text {
        content: String,
        span: SourceSpan,
    },
    Comment {
        content: String,
        span: SourceSpan,
    },
}

impl ConTeXtNode {
    pub fn span(&self) -> Option<&SourceSpan> {
        match self {
            ConTeXtNode::Command { span, .. } => Some(span),
            ConTeXtNode::StartStop { span, .. } => Some(span),
            ConTeXtNode::Text { span, .. } => Some(span),
            ConTeXtNode::Comment { span, .. } => Some(span),
            ConTeXtNode::Document { .. } => None,
        }
    }
    
    pub fn children(&self) -> Vec<&ConTeXtNode> {
        match self {
            ConTeXtNode::Document { preamble, body } => {
                preamble.iter().chain(body.iter()).collect()
            }
            ConTeXtNode::Command { arguments, .. } => arguments.iter().collect(),
            ConTeXtNode::StartStop { content, .. } => content.iter().collect(),
            ConTeXtNode::Text { .. } | ConTeXtNode::Comment { .. } => Vec::new(),
        }
    }
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum CommandStyle {
    TexStyle,
    ContextStyle,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub enum ArgumentStyle {
    Explicit,
    LineEnding,
    GroupScoped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub start_line: usize,
    pub start_col: usize,
}

impl From<LocatedSpan<&str>> for SourceSpan {
    fn from(span: LocatedSpan<&str>) -> Self {
        SourceSpan {
            start: span.location_offset(),
            end: span.location_offset() + span.fragment().len(),
            start_line: span.location_line() as usize,
            start_col: span.get_column(),
        }
    }
}

impl SourceSpan {
    pub fn line_col(&self) -> (usize, usize) {
        (self.start_line, self.start_col)
    }
    
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
    
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}
