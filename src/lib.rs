pub mod parser;
pub mod syntax;
pub mod highlight;
pub mod ast;
pub mod runtime;
pub mod ffi;
pub mod diagnostic;

use crate::highlight::HighlightKind;
use crate::ffi::{
    HighlightFfi, FfiRange, CompileResultFfi, DiagnosticFfi,
    ContextRuntimeHandle,
};

uniffi::include_scaffolding!("context");

#[cfg(test)]
mod tests {
    use super::ast::{ArgumentStyle, CommandStyle, ConTeXtNode};
    use super::parser;
    use nom_locate::LocatedSpan;

    #[cfg(test)]
    use pretty_assertions::assert_eq;

    macro_rules! assert_span {
        ($span:expr, $start:expr, $end:expr, $line:expr, $col:expr) => {
            assert_eq!($span.start, $start, "span start mismatch");
            assert_eq!($span.end, $end, "span end mismatch");
            assert_eq!($span.start_line, $line, "span line mismatch");
            assert_eq!($span.start_col, $col, "span column mismatch");
        };
    }

    #[test]
    fn parse_comment() {
        let input = "% This is a comment";
        
        let result = parser::parse_comment(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "");
        
        match node {
            ConTeXtNode::Comment { span, .. } => {
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected Comment node"),
        }
    }

    #[test]
    fn parse_context_style_command() {
        let input = r"\externalfigure[cow.pdf][scale=300]";
        
        let result = parser::parse_command(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "");
        
        match node {
            ConTeXtNode::Command { name, style, arg_style, options, arguments, span } => {
                assert_eq!(name, "externalfigure");
                assert_eq!(style, CommandStyle::ContextStyle);
                assert_eq!(arg_style, ArgumentStyle::Explicit);
                
                assert_eq!(options.len(), 1);
                assert_eq!(options.get("scale"), Some(&"300".to_string()));
                
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    ConTeXtNode::Text { content, span: arg_span } => {
                        assert_eq!(content, "cow.pdf");
                        // Argument span should be within the command span
                        assert!(arg_span.start >= span.start);
                        assert!(arg_span.end <= span.end);
                    }
                    _ => panic!("Expected Text argument"),
                }
                
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected Command node"),
        }
    }

    #[test]
    fn parse_tex_style_command_with_options() {
        let input = r"\framed[width=textwidth]{Hi}";
        
        let result = parser::parse_command(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "");
        
        match node {
            ConTeXtNode::Command { name, style, arg_style, options, arguments, span } => {
                assert_eq!(name, "framed");
                assert_eq!(style, CommandStyle::TexStyle);
                assert_eq!(arg_style, ArgumentStyle::Explicit);
                
                assert_eq!(options.len(), 1);
                assert_eq!(options.get("width"), Some(&"textwidth".to_string()));
                
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    ConTeXtNode::Text { content, .. } => {
                        assert_eq!(content, "Hi");
                    }
                    _ => panic!("Expected Text argument"),
                }
                
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected Command node"),
        }
    }

    #[test]
    fn parse_tex_style_command_no_options() {
        let input = r"\emph{Hey}";
        
        let result = parser::parse_command(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "");
        
        match node {
            ConTeXtNode::Command { name, style, arg_style, options, arguments, span } => {
                assert_eq!(name, "emph");
                assert_eq!(style, CommandStyle::TexStyle);
                assert_eq!(arg_style, ArgumentStyle::Explicit);
                assert!(options.is_empty());
                
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    ConTeXtNode::Text { content, .. } => {
                        assert_eq!(content, "Hey");
                    }
                    _ => panic!("Expected Text argument"),
                }
                
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected Command node"),
        }
    }

    #[test]
    fn parse_text() {
        let input = "Let \\im{x} be a variable.";
        
        let result = parser::parse_text(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "\\im{x} be a variable.");
        
        match node {
            ConTeXtNode::Text { content, span } => {
                assert_eq!(content, "Let ");
                assert_span!(span, 0, input.len(), 1, 1); 
            }
            _ => panic!("Expected Text node"),
        }
    }

    #[test]
    fn parse_startstop_environment() {
        let input = r"\startitemize
            \item Hello
        \stopitemize";

        let result = parser::parse_startstop(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), "");
        
        match node {
            ConTeXtNode::StartStop { name, options, content, span } => {
                assert_eq!(name, "itemize");
                assert!(options.is_empty());
                
                assert_eq!(content.len(), 3); 
                
                match (&content[0], &content[1], &content[2]) {
                    (
                        ConTeXtNode::Text { content: text1, .. },
                        ConTeXtNode::Command { name: cmd_name, arguments, .. },
                        ConTeXtNode::Text { content: text2, .. }
                    ) => {
                        assert_eq!(text1, "\n            ");
                        assert_eq!(cmd_name, "item");
                        assert_eq!(arguments.len(), 1);
                        match &arguments[0] {
                            ConTeXtNode::Text { content, .. } => assert_eq!(content, "Hello"),
                            _ => panic!("Expected text argument for item"),
                        }
                        assert_eq!(text2.trim(), "");
                    }
                    _ => panic!("Unexpected content structure"),
                }
                
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected StartStop node"),
        }
    }

    #[test]
    fn parse_document_structure_without_preamble() {
        let input = r"\startdocument
                \chapter{Introduction}
            \stopdocument";

        let result = parser::parse_document(input);
        assert!(result.is_ok());
        
        let document = result.unwrap();
        match document {
            ConTeXtNode::Document { preamble, body } => {
                assert!(preamble.is_empty());
                assert_eq!(body.len(), 1);
                
                match &body[0] {
                    ConTeXtNode::StartStop { name, options, content, .. } => {
                        assert_eq!(name, "document");
                        assert!(options.is_empty());
                        
                        assert_eq!(content.len(), 3);
                        
                        match &content[1] {
                            ConTeXtNode::Command { name, arguments, .. } => {
                                assert_eq!(name, "chapter");
                                assert_eq!(arguments.len(), 1);
                                match &arguments[0] {
                                    ConTeXtNode::Text { content, .. } => {
                                        assert_eq!(content, "Introduction");
                                    }
                                    _ => panic!("Expected text argument"),
                                }
                            }
                            _ => panic!("Expected chapter command"),
                        }
                    }
                    _ => panic!("Expected StartStop node"),
                }
            }
            _ => panic!("Expected Document node"),
        }
    }

    #[test]
    fn parse_document_structure() {
        let input = r"\setupbodyfont[palatino, 12pt]
            \starttext
                \startsection[title=Introduction]
                    Hello
                \stopsection
            \stoptext";

        let result = parser::parse_document(input);
        assert!(result.is_ok());
        
        let document = result.unwrap();
        match document {
            ConTeXtNode::Document { preamble, body } => {
                assert_eq!(preamble.len(), 2);
                
                match &preamble[0] {
                    ConTeXtNode::Command { name, style, arguments, .. } => {
                        assert_eq!(name, "setupbodyfont");
                        assert_eq!(*style, CommandStyle::ContextStyle);
                        assert_eq!(arguments.len(), 1);
                        match &arguments[0] {
                            ConTeXtNode::Text { content, .. } => {
                                assert_eq!(content, "palatino, 12pt");
                            }
                            _ => panic!("Expected text argument"),
                        }
                    }
                    _ => panic!("Expected setupbodyfont command"),
                }
                
                assert_eq!(body.len(), 3);
                
                match &body[1] {
                    ConTeXtNode::StartStop { name, options, content, .. } => {
                        assert_eq!(name, "section");
                        assert_eq!(options.len(), 1);
                        assert_eq!(options.get("title"), Some(&"Introduction".to_string()));
                        
                        assert_eq!(content.len(), 1);
                        match &content[0] {
                            ConTeXtNode::Text { content, .. } => {
                                assert!(content.contains("Hello"));
                            }
                            _ => panic!("Expected text content"),
                        }
                    }
                    _ => panic!("Expected section StartStop"),
                }
            }
            _ => panic!("Expected Document node"),
        }
    }

    #[test]
    fn parse_group_scoped_command() {
        let input = r"\bf{This is bold} and this is not";
        
        let result = parser::parse_command(LocatedSpan::new(input));
        assert!(result.is_ok());
        
        let (remaining, node) = result.unwrap();
        assert_eq!(*remaining.fragment(), " and this is not");
        
        match node {
            ConTeXtNode::Command { name, style, arg_style, options, arguments, span } => {
                assert_eq!(name, "bf");
                assert_eq!(style, CommandStyle::TexStyle);
                assert_eq!(arg_style, ArgumentStyle::GroupScoped);
                assert!(options.is_empty());
                
                assert_eq!(arguments.len(), 1);
                match &arguments[0] {
                    ConTeXtNode::Text { content, .. } => {
                        assert_eq!(content, "This is bold");
                    }
                    _ => panic!("Expected Text argument"),
                }
                
                assert_span!(span, 0, input.len(), 1, 1);
            }
            _ => panic!("Expected Command node"),
        }
    }
}
