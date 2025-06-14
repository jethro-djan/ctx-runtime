pub mod ast;
pub mod parser;

#[cfg(test)]
mod tests {
    use super::ast::{Node, Command, CommandStyle};
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
                    options: Vec::new(),
                    arguments: vec![Node::Text("Hey".to_string())],
                })
            ))
        );
    }
}
