use nom::{
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{alpha1, char, not_line_ending, multispace0},
    combinator::{map, opt, recognize, verify},
    multi::{many0, many_till},
    sequence::{delimited, preceded, terminated},
    IResult, Parser,
};
use std::collections::HashMap;

use crate::ast::{ConTeXtNode, SourceSpan, ArgumentStyle, CommandStyle};

pub fn parse_document(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, nodes) = many0(parse_node).parse(input)?;
    Ok((input, ConTeXtNode::Document {
        preamble: Vec::new(), // Will handle later
        body: nodes,
    }))
}

pub fn parse_node(input: &str) -> IResult<&str, ConTeXtNode> {
    alt((
        parse_comment,
        parse_startstop,
        parse_command,
        parse_text,
    )).parse(input)
}

pub fn parse_command(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, _) = char('\\')(input)?;
    let (input, name) = alpha1(input)?;
    
    let (style, arg_style) = match name {
        "item" => (CommandStyle::TexStyle, ArgumentStyle::LineEnding),
        "em" | "bf" => (CommandStyle::TexStyle, ArgumentStyle::GroupScoped),
        _ => (CommandStyle::TexStyle, ArgumentStyle::Explicit),
    };

    let (input, options) = opt(parse_options).parse(input)?;
    let (input, arguments) = match arg_style {
        ArgumentStyle::LineEnding => parse_line_ending_args(input)?,
        ArgumentStyle::GroupScoped => parse_group_scoped_args(input)?,
        ArgumentStyle::Explicit => parse_explicit_args(input)?,
    };

    Ok((input, ConTeXtNode::Command {
        name: name.to_string(),
        style,
        arg_style,
        options: options.unwrap_or_default(),
        arguments,
        span: dummy_span(), // Implement proper span tracking
    }))
}

pub fn parse_startstop(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, _) = tag("\\start")(input)?;
    let (input, name) = alpha1(input)?;
    let (input, options) = opt(parse_options).parse(input)?;
    
    let stop_tag = format!("\\stop{}", name);
    let (input, (content, _)) = many_till(
        parse_node,
        tag(&*stop_tag)
    ).parse(input)?;

    Ok((input, ConTeXtNode::StartStop {
        name: name.to_string(),
        options: options.unwrap_or_default(),
        content,
        span: dummy_span(), // Implement proper span tracking
    }))
}

pub fn parse_comment(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, comment) = preceded(
        char('%'),
        not_line_ending
    ).parse(input)?;
    
    Ok((input, ConTeXtNode::Comment {
        content: comment.trim().to_string(),
        span: dummy_span(),
    }))
}

pub fn parse_text(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, text) = verify(
        take_while1(|c| c != '\\' && c != '%' && c != '{' && c != '}'),
        |s: &str| !s.is_empty()
    ).parse(input)?;
    
    Ok((input, ConTeXtNode::Text {
        content: text.to_string(),
        span: dummy_span(),
    }))
}

// Argument parsers
pub fn parse_explicit_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    opt(delimited(
        char('{'),
        many0(parse_node),
        char('}'),
    )).parse(input).map(|(i, v)| (i, v.unwrap_or_default()))
}

pub fn parse_line_ending_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    let (input, text) = terminated(
        take_until("\n"),
        char('\n')
    ).parse(input)?;
    
    Ok((input, vec![ConTeXtNode::Text {
        content: text.trim().to_string(),
        span: dummy_span(),
    }]))
}

pub fn parse_group_scoped_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    many0(parse_node).parse(input)
}

pub fn parse_options(input: &str) -> IResult<&str, HashMap<String, String>> {
    delimited(
        char('['),
        map(
            take_until("]"),
            |s: &str| {
                s.split(',')
                 .filter_map(|pair| {
                     let mut kv = pair.splitn(2, '=');
                     Some((kv.next()?.trim().to_string(), 
                          kv.next().unwrap_or("true").trim().to_string()))
                 })
            }
                 .collect()
            ),
        char(']'),
    ).parse(input)
}

pub fn dummy_span() -> SourceSpan {
    SourceSpan { start: 0, end: 0, line: 0, column: 0 }
}
