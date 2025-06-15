pub mod parser;

pub mod ast {
    #[derive(Debug, PartialEq, Clone)]
    pub enum Node {
        Command(Command), 
        StartStop {
            name: String,
            options: Vec<String>,
            content: Vec<Node>,
        },
        Text(String),
        Comment(String),
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
        pub arguments: Vec<Node>,
    }

    #[derive(Debug, PartialEq, Clone)]
    pub enum ArgumentStyle {
        Explicit,
        LineEnding,
        GroupScoped,
    }
}

#[cfg(test)]
mod tests {
    use super::ast::{Node, Command, CommandStyle, ArgumentStyle};
    use super::parser;

    #[test]
    fn parse_comment() {
        assert_eq!(
            parser::ctx_comment("% This is a comment"), 
            Ok((
                "", 
                Node::Comment("This is a comment".to_string())
            ))
        );
    }

    #[test]
    fn parse_ctx_style_command() {
        assert_eq!(
            parser::ctx_command(r"\externalfigure[cow.pdf][scale=300]"), 
            Ok((
                "", 
                Node::Command(Command {
                    name: "externalfigure".to_string(),
                    style: CommandStyle::ContextStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options: vec!["scale=300".to_string()],
                    arguments: vec![Node::Text("cow.pdf".to_string())],
                })
            ))
        );
    }

    #[test]
    fn parse_tex_style_command() {
        assert_eq!(
            parser::ctx_command(r"\framed[width=textwidth]{Hi}"), 
            Ok((
                "", 
                Node::Command(Command {
                    name: "framed".to_string(),
                    style: CommandStyle::TexStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options: vec!["width=textwidth".to_string()],
                    arguments: vec![Node::Text("Hi".to_string())],
                })
            ))
        );
    }

    #[test]
    fn parse_tex_style_command_no_options() {
        assert_eq!(
            parser::ctx_command(r"\emph{Hey}"), 
            Ok((
                "", 
                Node::Command(Command {
                    name: "emph".to_string(),
                    style: CommandStyle::TexStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options: Vec::new(),
                    arguments: vec![Node::Text("Hey".to_string())],
                })
            ))
        );
    }

    #[test]
    fn parse_text() {
        assert_eq!(
            parser::ctx_text(r"Let \im{x} be a variable."), 
            Ok((
                r"\im{x} be a variable.", 
                Node::Text("Let ".to_string())
            ))
        );
    }
    #[test]
    fn parse_startstop() {
        assert_eq!(
            parser::ctx_startstop(r"
                \startitemize
                    \item Hello
                \stopitemize
            "), 
            Ok((
                "", 
                Node::StartStop{
                    name: "itemize".to_string(),
                    options: Vec::new(),
                    content: 
                        vec![Node::Command(
                            Command {
                                name: "item".to_string(),
                                style: CommandStyle::TexStyle,
                                arg_style: ArgumentStyle::LineEnding,
                                options: Vec::new(),
                                arguments: vec![Node::Text("Hello".to_string())],
                            }
                        )
                    ]
                }
            ))
        );
    }
}
