pub mod ast;
pub mod parser;
pub mod syntax;
pub mod highlight;

use highlight::Highlight;

pub struct ConTeXtEngine {
    syntax_tree: Option<syntax::SyntaxNode>,
}

impl ConTeXtEngine {
    pub fn update(&mut self, text: &str) {
        let (_, document_node) = parser::parse_document(text).unwrap();
        self.syntax_tree = Some(syntax::SyntaxNode::new_root(syntax::ast_to_rowan(document_node)));
    }

    pub fn highlights(&self) -> Vec<Highlight> {
        self.syntax_tree.as_ref()
            .map(|tree| highlight::run(tree))
            .unwrap_or_default()
    }
}
    
#[cfg(test)]
mod tests {
    use super::ast::{ArgumentStyle, CommandStyle, ConTeXtNode, SourceSpan};
    use super::parser;
    use std::collections::HashMap;

    #[cfg(test)]
    use pretty_assertions::assert_eq;

    fn test_span() -> SourceSpan {
        SourceSpan { start: 0, end: 0 }
    }

    #[test]
    fn parse_comment() {
        let input = "% This is a comment";
        let expected = ConTeXtNode::Comment {
            content: "This is a comment".to_string(),
            span: SourceSpan { start: 0, end: 0 },
        };

        assert_eq!(parser::parse_comment(input), Ok(("", expected)));
    }

    #[test]
    fn parse_context_style_command() {
        let input = r"\externalfigure[cow.pdf][scale=300]";
        let mut options = HashMap::new();
        options.insert("scale".to_string(), "300".to_string());

        let expected = ConTeXtNode::Command {
            name: "externalfigure".to_string(),
            style: CommandStyle::ContextStyle,
            arg_style: ArgumentStyle::Explicit,
            options,
            arguments: vec![ConTeXtNode::Text {
                content: "cow.pdf".to_string(),
                span: test_span(),
            }],
            span: test_span(),
        };

        assert_eq!(parser::parse_command(input), Ok(("", expected)));
    }

    #[test]
    fn parse_tex_style_command_with_options() {
        let input = r"\framed[width=textwidth]{Hi}";
        let mut options = HashMap::new();
        options.insert("width".to_string(), "textwidth".to_string());

        let expected = ConTeXtNode::Command {
            name: "framed".to_string(),
            style: CommandStyle::TexStyle,
            arg_style: ArgumentStyle::Explicit,
            options,
            arguments: vec![ConTeXtNode::Text {
                content: "Hi".to_string(),
                span: test_span(),
            }],
            span: test_span(),
        };

        assert_eq!(parser::parse_command(input), Ok(("", expected)));
    }

    #[test]
    fn parse_tex_style_command_no_options() {
        let input = r"\emph{Hey}";
        let expected = ConTeXtNode::Command {
            name: "emph".to_string(),
            style: CommandStyle::TexStyle,
            arg_style: ArgumentStyle::Explicit,
            options: HashMap::new(),
            arguments: vec![ConTeXtNode::Text {
                content: "Hey".to_string(),
                span: test_span(),
            }],
            span: test_span(),
        };

        assert_eq!(parser::parse_command(input), Ok(("", expected)));
    }

    #[test]
    fn parse_text() {
        let input = "Let \\im{x} be a variable.";
        let expected = ConTeXtNode::Text {
            content: "Let ".to_string(),
            span: SourceSpan { start: 0, end: 0 },
        };

        assert_eq!(
            parser::parse_text(input),
            Ok(("\\im{x} be a variable.", expected))
        );
    }

    #[test]
    fn parse_startstop_environment() {
        let input = r"\startitemize
            \item Hello
        \stopitemize";

        let expected = ConTeXtNode::StartStop {
            name: "itemize".to_string(),
            options: HashMap::new(),
            content: vec![
                ConTeXtNode::Text {
                    content: "\n            ".to_string(),
                    span: parser::dummy_span(),
                },
                ConTeXtNode::Command {
                    name: "item".to_string(),
                    style: CommandStyle::TexStyle,
                    arg_style: ArgumentStyle::LineEnding,
                    options: HashMap::new(),
                    arguments: vec![ConTeXtNode::Text {
                        content: "Hello".to_string(),
                        span: parser::dummy_span(),
                    }],
                    span: parser::dummy_span(),
                },
                ConTeXtNode::Text {
                    content: "        ".to_string(),
                    span: parser::dummy_span(),
                },
            ],
            span: parser::dummy_span(),
        };

        assert_eq!(parser::parse_startstop(input), Ok(("", expected)));
    }

    #[test]
    fn parse_document_structure_without_preamble() {
        let input = r"\startdocument
                \chapter{Introduction}
            \stopdocument";

        let expected = ConTeXtNode::Document {
            preamble: Vec::new(),
            body: vec![ConTeXtNode::StartStop {
                name: "document".to_string(),
                options: HashMap::new(),
                content: vec![
                    ConTeXtNode::Text {
                        content: "\n                ".to_string(),
                        span: parser::dummy_span(),
                    },
                    ConTeXtNode::Command {
                        name: "chapter".to_string(),
                        style: CommandStyle::TexStyle,
                        arg_style: ArgumentStyle::Explicit,
                        options: HashMap::new(),
                        arguments: vec![ConTeXtNode::Text {
                            content: "Introduction".to_string(),
                            span: test_span(),
                        }],
                        span: test_span(),
                    },
                    ConTeXtNode::Text {
                        content: "\n            ".to_string(),
                        span: parser::dummy_span(),
                    },
                ],
                span: SourceSpan { start: 0, end: 79 },
            }],
        };

        assert_eq!(parser::parse_document(input), Ok(("", expected)));
    }

    #[test]
    fn parse_document_structure() {
        let input = r"\setupbodyfont[palatino, 12pt]
            \starttext
                \startsection[title=Introduction]
                    Hello
                \stopsection
            \stoptext";
        let mut section_options = HashMap::new();
        section_options.insert("title".to_string(), "Introduction".to_string());

        let expected = ConTeXtNode::Document {
            preamble: vec![
                ConTeXtNode::Command {
                    name: "setupbodyfont".to_string(),
                    style: CommandStyle::ContextStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options: HashMap::new(),
                    arguments: vec![ConTeXtNode::Text {
                        content: "palatino, 12pt".to_string(),
                        span: test_span(),
                    }],
                    span: test_span(),
                },
                ConTeXtNode::Text {
                    content: "\n            ".to_string(),
                    span: parser::dummy_span(),
                },
            ],
            body: vec![
                ConTeXtNode::Text {
                    content: "\n                ".to_string(),
                    span: parser::dummy_span(),
                },
                ConTeXtNode::StartStop {
                    name: "section".to_string(),
                    options: section_options,
                    content: vec![ConTeXtNode::Text {
                        content: "\n                    Hello\n                ".to_string(),
                        span: parser::dummy_span(),
                    }],
                    span: parser::dummy_span(),
                },
                ConTeXtNode::Text {
                    content: "\n            ".to_string(),
                    span: parser::dummy_span(),
                },
            ],
        };

        assert_eq!(parser::parse_document(input), Ok(("", expected)));
    }

    #[test]
    fn parse_group_scoped_command() {
        let input = r"\bf{This is bold} and this is not";
        let expected = ConTeXtNode::Command {
            name: "bf".to_string(),
            style: CommandStyle::TexStyle,
            arg_style: ArgumentStyle::GroupScoped,
            options: HashMap::new(),
            arguments: vec![ConTeXtNode::Text {
                content: "This is bold".to_string(),
                span: test_span(),
            }],
            span: test_span(),
        };

        assert_eq!(
            parser::parse_command(input),
            Ok((" and this is not", expected))
        );
    }
}
