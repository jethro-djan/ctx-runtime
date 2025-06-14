use nom::{ 
    bytes::complete::{tag, take_until, take_till}, 
    character::complete::{not_line_ending, alpha1},
    combinator::{opt, map_res, map},
    sequence::{preceded, delimited, separated_pair, tuple}, 
    multi::{many0, many1, separated_list1},
    IResult, 
    Parser,
    branch::alt,
    error,
};
use crate::ast::{Node, CommandStyle, Command};

pub fn ctx_command(input: &str) -> IResult<&str, Node> {
    let (input, _) = tag("\\")(input)?;
    let (input, command_name) = alpha1(input)?;

    println!("input: {:?}", input);
    if let Ok((remaining, (arg, opt))) = parse_context_style_args(input) {
        if !remaining.starts_with('{') {
            return Ok((remaining, Node::Command(Command {
                name: command_name.to_string(),
                style: CommandStyle::ContextStyle,
                options: opt.unwrap_or_default(),
                arguments: vec![Node::Text(arg.to_string())],
            })));
        }
    }
    println!(": {:?}", input);

    let (input, maybe_options) = opt(parse_command_options).parse(input)?;
    let (input, maybe_args) = opt(parse_group).parse(input)?;

    let (input, options) = if maybe_args.is_some() && maybe_options.is_none() {
        opt(parse_command_options).parse(input)?
    } else {
        (input, maybe_options)
    };

    Ok((input, Node::Command(Command {
        name: command_name.to_string(),
        style: CommandStyle::TexStyle,
        options: options.unwrap_or_default(),
        arguments: maybe_args.unwrap_or_default(),
    })))
}

// pub fn ctx_startstop(input: &str) -> IResult<&str, Node> {
// }

pub fn ctx_text(input: &str) -> IResult<&str, Node> {
    let (input, text) = take_till(|c| c == '\\' || c == '}')(input)?;
    if text.is_empty() {
        Err(nom::Err::Error(nom::error::Error::new(input, nom::error::ErrorKind::TakeWhile1)))
    } else {
        Ok((input, Node::Text(text.to_string())))
    }
}

pub fn ctx_comment(input: &str) -> IResult<&str, Node> {
    let (input, comment) = preceded(tag("%"), not_line_ending).parse(input)?;
    Ok((input, Node::Comment(comment.trim().to_string())))
}

pub fn ctx_code(input: &str) -> IResult<&str, Vec<Node>> {
    many0(alt((
        ctx_command,
        ctx_comment,
        ctx_text,
    ))).parse(input)
}

fn parse_context_style_args(input: &str) -> IResult<&str, (&str, Option<Vec<String>>)> {
    tuple((
        delimited(tag("["), take_until("]"), tag("]")),
        opt(parse_command_options),
    )).parse(input)
}

pub fn parse_command_options(input: &str) -> IResult<&str, Vec<String>> {
    delimited(
        tag("["),
        separated_list1(
            tag("]["),
            map(
                take_until("]"),
                |s: &str| s.to_string()
            )
        ),
        tag("]"),
    ).parse(input)
}

pub fn parse_group(input: &str) -> IResult<&str, Vec<Node>> {
    delimited(
        tag("{"),
        many1(alt((
            ctx_command,
            ctx_comment,
            ctx_text,
        ))),
        tag("}"),
    ).parse(input)
}


