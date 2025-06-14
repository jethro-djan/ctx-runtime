#[derive(Debug, PartialEq)]
pub enum Node {
    Command(Command), 
    Environment {
        name: String,
        options: Vec<String>,
        content: Vec<Node>,
    },
    Text(String),
    Comment(String),
}

#[derive(Debug, PartialEq)]
pub enum CommandStyle {
    TexStyle,
    ContextStyle,
}

#[derive(Debug, PartialEq)]
pub struct Command {
    pub name: String,
    pub style: CommandStyle,
    pub options: Vec<String>,
    pub arguments: Vec<Node>,
}
