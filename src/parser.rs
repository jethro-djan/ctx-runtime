use crate::lexer::Token;
use crate::syntax::{SyntaxKind, SyntaxTreeBuilder, SyntaxTree};
use bumpalo::Bump;
use logos::Logos;

pub fn parse_text(text: &str) -> SyntaxTree {
    let arena = Bump::new();
    
    let lexer = Token::lexer(text);
    let mut tokens: Vec<_> = lexer
        .spanned()
        .map(|(token, span)| (token.unwrap(), span))
        .collect();
    tokens.reverse();
    
    let mut builder = SyntaxTreeBuilder::new(&arena);
    
    parse_document(text, &mut tokens, &mut builder);
    
    builder.finish()
}

fn parse_document<'a>(
    source: &'a str,  // Added lifetime 'a
    tokens: &mut Vec<(Token, logos::Span)>,
    builder: &mut SyntaxTreeBuilder<'a>,
) {
    builder.start_node(SyntaxKind::Document);
    
    while let Some((token, span)) = tokens.pop() {
        match token {
            Token::StartText | Token::StartDocument => {
                parse_environment(source, tokens, builder);
            }
            Token::Command => {
                parse_command(source, tokens, builder);
            }
            Token::Text => {
                let text = &source[span.start..span.end];
                builder.token(SyntaxKind::Text, text);
            }
            Token::Comment => {
                let comment = &source[span.start..span.end];
                builder.token(SyntaxKind::Comment, comment);
            }
            _ => {}
        }
    }
    
    builder.finish_node();
}

fn parse_environment<'a>(
    source: &'a str,  // Added lifetime 'a
    tokens: &mut Vec<(Token, logos::Span)>,
    builder: &mut SyntaxTreeBuilder<'a>,
) {
    builder.start_node(SyntaxKind::Environment);
    
    while let Some((token, span)) = tokens.pop() {
        match token {
            Token::StopEnv | Token::StopText | Token::StopDocument => break,
            Token::Command => parse_command(source, tokens, builder),
            Token::Text => {
                let text = &source[span.start..span.end];
                builder.token(SyntaxKind::Text, text);
            },
            Token::Comment => {
                let comment = &source[span.start..span.end];
                builder.token(SyntaxKind::Comment, comment);
            },
            _ => {}
        }
    }
    
    builder.finish_node();
}

fn parse_command<'a>(
    source: &'a str,  // Added lifetime 'a
    tokens: &mut Vec<(Token, logos::Span)>,
    builder: &mut SyntaxTreeBuilder<'a>,
) {
    builder.start_node(SyntaxKind::Command);
    
    while let Some((token, span)) = tokens.pop() {
        match token {
            Token::Options => {
                let options = &source[span.start..span.end];
                builder.token(SyntaxKind::Options, options);
            }
            Token::BraceOpen => {
                parse_argument(source, tokens, builder);
            }
            _ => {
                tokens.push((token, span));
                break;
            }
        }
    }
    
    builder.finish_node();
}

fn parse_argument<'a>(
    source: &'a str,  // Added lifetime 'a
    tokens: &mut Vec<(Token, logos::Span)>,
    builder: &mut SyntaxTreeBuilder<'a>,
) {
    builder.start_node(SyntaxKind::Argument);
    
    while let Some((token, span)) = tokens.pop() {
        match token {
            Token::BraceClose => break,
            Token::Command => parse_command(source, tokens, builder),
            Token::Text => {
                let text = &source[span.start..span.end];
                builder.token(SyntaxKind::Text, text);
            },
            Token::Comment => {
                let comment = &source[span.start..span.end];
                builder.token(SyntaxKind::Comment, comment);
            },
            _ => {}
        }
    }
    
    builder.finish_node();
}
