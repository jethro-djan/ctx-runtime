use nom::{ 
    bytes::complete::{tag, take_until}, 
    character::complete::{not_line_ending, alpha1},
    combinator::{opt, map_res},
    sequence::{preceded, tuple}, 
    multi::many0,
    IResult, 
    Parser,
    error,
};
use crate::ast::{Node, CommandStyle};

pub fn ctx_command(input: &str) -> IResult<&str, Node>> {
    let (input, _) = tag("\\")(input)?;
    let (input, command_name) = alpha1(input)?;

    if let Ok(remaining, (arg, opt)) = parse_context_style_args(input) {
        return Ok((remaining, Node::Command(Command {
            name: command_name.to_string(), 
            style: CommandStyle::TexStyle,
            arguments: opt.unwrap_or_default(), 
            options: vec![Node::Text(arg), 
        })));
    }

    let (input, opt) = opt(parse_command_options).parse(input)?;
    let (input, args) = opt(parse_group)
        .map(|args| args.unwrap_or_default())
        .parse(input)?;

    Ok((input, Node::Command {
        name: command_name.to_string(), 
        style: CommandStyle::TexStyle,
        arguments: opt.unwrap_or_default(), 
        options: args, 
    }))
}

pub fn ctx_text(input: &str) -> IResult<&str, Node>> {
    let (input, text) = take_until("\\")(input)?;
    if text.is_empty() {
        Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::TakeUntil)))
    } else {
        Ok((input, Node::Text(text.to_string())))
    }
}

pub fn ctx_comment(input: &str) -> IResult<&str, Node>> {
    let (input, comment) = preceded(tag("%"), not_line_ending)(input)?;
    Ok(input, Node::Comment(comment.trim().to_string())
}

pub fn ctx_code(input: &str) -> IResult<&str, Vec<Node>> {
    many0(alt((
        ctx_command,
        ctx_comment,
        ctx_text,
    ))).parse(input)
}

pub fn parse_context_style_args(input: &str) -> IResult<&str, (String, Option<Vec<String>>)> {
    tuple((
        delimited(tag("["), take_until("]"), tag("]")),
        opt(parse_command_options),
    )).parse(input)
}

pub fn parse_command_options(input: &str) -> IResult<&str, Vec<String>> {
    delimited(
        tag("["),
        many1(map(
            take_until("]"),
            |s: &str| s.to_string()
        )),
        tag("]"),
    ).parse(input)
}

pub fn parse_group(input: &str) -> IResult<&str, Vec<Node>> {
        tag("["),
        many1(alt((
            ctx_command,
            ctx_comment,
            ctx_text,
        ))),
        tag("]"),
    ).parse(input)
}
