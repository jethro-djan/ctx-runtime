use logos::Logos;

#[derive(Logos, Debug, PartialEq, Clone, Copy)]
#[logos(skip r"[ \t\n]+")]
pub enum Token {
    #[token("\\starttext")]
    StartText,
    
    #[token("\\stoptext")]
    StopText,
    
    #[token("\\startdocument")]
    StartDocument,
    
    #[token("\\stopdocument")]
    StopDocument,
    
    #[regex(r"\\start[a-zA-Z]+")]
    StartEnv,
    
    #[regex(r"\\stop[a-zA-Z]+")]
    StopEnv,
    
    #[regex(r"\\[a-zA-Z]+")]
    Command,
    
    #[regex(r"\[[^\]]*\]")]
    Options,
    
    #[regex(r"[^{}\[\]\\% \t\n]+")]
    Text,
    
    #[token("{")]
    BraceOpen,
    
    #[token("}")]
    BraceClose,
    
    #[regex(r"%[^\n]*")]
    Comment,
    
    // #[logos(skip r"[ \t\n\f]+")]
    // Error,
}
