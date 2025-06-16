use std::collections::HashMap;

#[derive(Debug, PartialEq, Clone)]
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

#[derive(Debug, PartialEq, Clone)]
pub enum CommandStyle {
    TexStyle,
    ContextStyle,
}

#[derive(Debug, PartialEq, Clone)]
pub struct Command {
    pub name: String,
    pub style: CommandStyle,
    pub arg_style: ArgumentStyle,
    pub options: Vec<String>,
    pub arguments: Vec<ConTeXtNode>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum ArgumentStyle {
    Explicit,
    LineEnding,
    GroupScoped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

impl SourceSpan {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn to_line_col(&self, source: &str) -> (usize, usize) {
        let mut line = 0;
        let mut col = 0;
        for (i, ch) in source.char_indices() {
            if i >= self.start {
                break;
            }
            if ch == '\n' {
                line += 1;
                col = 0;
            } else {
                col += 1;
            }
        }
        (line, col)
    }
}
