use std::collections::HashMap;
use nom_locate::LocatedSpan;

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
    /// Get the line and column of the start position
    pub fn line_col(&self) -> (usize, usize) {
        (self.start_line, self.start_col)
    }
    
    /// Get the byte range of this span
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start..self.end
    }
    
    /// Get the length of this span in bytes
    pub fn len(&self) -> usize {
        self.end - self.start
    }
    
    /// Check if this span is empty
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

// use std::collections::HashMap;
// 
// #[derive(Debug, PartialEq, Clone)]
// pub enum ConTeXtNode {
//     Document {
//         preamble: Vec<ConTeXtNode>,
//         body: Vec<ConTeXtNode>,
//     },
//     Command {
//         name: String,
//         style: CommandStyle,
//         arg_style: ArgumentStyle,
//         options: HashMap<String, String>,
//         arguments: Vec<ConTeXtNode>,
//         span: SourceSpan,
//     },
//     StartStop {
//         name: String,
//         options: HashMap<String, String>,
//         content: Vec<ConTeXtNode>,
//         span: SourceSpan,
//     },
//     Text {
//         content: String,
//         span: SourceSpan,
//     },
//     Comment {
//         content: String,
//         span: SourceSpan,
//     },
// }
// 
// #[derive(Debug, PartialEq, Clone)]
// pub enum CommandStyle {
//     TexStyle,
//     ContextStyle,
// }
// 
// #[derive(Debug, PartialEq, Clone)]
// pub struct Command {
//     pub name: String,
//     pub style: CommandStyle,
//     pub arg_style: ArgumentStyle,
//     pub options: Vec<String>,
//     pub arguments: Vec<ConTeXtNode>,
// }
// 
// #[derive(Debug, PartialEq, Clone)]
// pub enum ArgumentStyle {
//     Explicit,
//     LineEnding,
//     GroupScoped,
// }
// 
// #[derive(Debug, Clone, PartialEq)]
// pub struct SourceSpan {
//     pub start: usize,
//     pub end: usize,
// }
// 
// 
// impl From<Span<'_>> for SourceSpan {
//     fn from(span: Span) -> Self {
//         SourceSpan {
//             start: span.location_offset(),
//             end: span.location_offset() + span.fragment().len(),
//         }
//     }
// }
// 
// impl SourceSpan {
//     pub fn new(start: usize, end: usize) -> Self {
//         Self { start, end }
//     }
// 
//     pub fn to_line_col(&self, source: &str) -> (usize, usize) {
//         let mut line = 0;
//         let mut col = 0;
//         for (i, ch) in source.char_indices() {
//             if i >= self.start {
//                 break;
//             }
//             if ch == '\n' {
//                 line += 1;
//                 col = 0;
//             } else {
//                 col += 1;
//             }
//         }
//         (line, col)
//     }
// }
