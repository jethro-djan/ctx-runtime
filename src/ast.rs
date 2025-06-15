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
