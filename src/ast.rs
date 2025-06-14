#[derive(Debug, PartialEq)]
pub enum Node {
    Command(Command), 
    Environment {
        name: String,
        options: Vec<String>,
        content: Vec<Node>,
    }
    Text(String),
    Comment(String),
}

pub enum CommandStyle {
    TexStyle,
    ContextSyle,
}

#[derive(Debug, PartialEq)]
pub enum Command {
    pub name: String,
    pub style: CommandStyle,
    options: Vec<String>,
    arguments: Vec<Node>,
}
