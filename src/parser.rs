use nom::{ 
    bytes::complete::{tag, take_until, take_till}, 
    character::complete::{not_line_ending, alpha1, multispace0},
    combinator::{opt, map_res, map, rest},
    sequence::{preceded, delimited, separated_pair, tuple, terminated}, 
    multi::{many0, many1, separated_list1},
    IResult, 
    Parser,
    branch::alt,
    error,
};

use crate::ast::{Node, CommandStyle, Command, ArgumentStyle};

pub fn ctx_command(input: &str) -> IResult<&str, Node> {
    let (input, _) = tag("\\")(input)?;
    let (input, command_name) = alpha1(input)?;

    if let Ok((remaining, (arg, opt))) = parse_context_style_args(input) {
        if !remaining.starts_with('{') {
            return Ok((remaining, Node::Command(Command {
                name: command_name.to_string(),
                style: CommandStyle::ContextStyle,
                arg_style: ArgumentStyle::Explicit,
                options: opt.unwrap_or_default(),
                arguments: vec![Node::Text(arg.to_string())],
            })));
        }
    }

    if is_line_ending_command(&command_name) {
        return parse_line_ending_command(input, command_name.to_string());
    }

    if is_group_scoped_command(&command_name) {
        return parse_group_scoped_command(input, command_name.to_string());
    }

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
        arg_style: ArgumentStyle::Explicit,
        options: options.unwrap_or_default(),
        arguments: maybe_args.unwrap_or_default(),
    })))
}

pub fn ctx_startstop(input: &str) -> IResult<&str, Node> {
    let (input, _) = multispace0(input)?;
    let (input, _) = tag("\\start")(input)?;
    let (input, env_name) = alpha1(input)?;
    
    let (input, options) = opt(parse_command_options).parse(input)?;
    
    let (input, _) = multispace0(input)?;
    
    let stop_tag = format!("\\stop{}", env_name);
    let (input, content_str) = take_until(stop_tag.as_str()).parse(input)?;
    
    let (input, _) = tag(&*stop_tag)(input)?;

    let (input, _) = multispace0(input)?;
    
    let (_, content) = ctx_code(content_str.trim())?;
    
    Ok((input, Node::StartStop {
        name: env_name.to_string(),
        options: options.unwrap_or_default(),
        content,
    }))
}

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

fn is_line_ending_command(name: &str) -> bool {
    matches!(name, "item")
}

fn is_group_scoped_command(name: &str) -> bool {
    matches!(name, "it" | "bf" | "tt" | "em" | "rm" | "sf" | "sc" | "sl")
}

fn parse_line_ending_command(input: &str, command_name: String) -> IResult<&str, Node> {
    let (input, _) = multispace0(input)?;
    
    let (input, text) = alt((
        terminated(take_until("\n"), tag("\n")),
        rest 
    )).parse(input)?;
    
    let arguments = if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![Node::Text(text.trim().to_string())]
    };

    Ok((input, Node::Command(Command {
        name: command_name,
        style: CommandStyle::TexStyle,
        arg_style: ArgumentStyle::LineEnding,
        options: Vec::new(),
        arguments,
    })))
}

pub fn parse_group(input: &str) -> IResult<&str, Vec<Node>> {
    delimited(
        tag("{"),
        parse_group_content,
        tag("}"),
    ).parse(input)
}

fn parse_group_content(input: &str) -> IResult<&str, Vec<Node>> {
    let mut nodes = Vec::new();
    let mut remaining = input;
    
    while !remaining.is_empty() && !remaining.starts_with('}') {
        if let Ok((new_remaining, node)) = ctx_command(remaining) {
            if let Node::Command(ref cmd) = node {
                if cmd.arg_style == ArgumentStyle::GroupScoped {
                    let (final_remaining, scoped_content) = parse_group_content(new_remaining)?;
                    
                    let scoped_command = Node::Command(Command {
                        name: cmd.name.clone(),
                        style: cmd.style.clone(),
                        arg_style: cmd.arg_style.clone(),
                        options: cmd.options.clone(),
                        arguments: scoped_content,
                    });
                    
                    nodes.push(scoped_command);
                    remaining = final_remaining;
                    break; 
                }
            }
            nodes.push(node);
            remaining = new_remaining;
        }
        else if let Ok((new_remaining, node)) = ctx_comment(remaining) {
            nodes.push(node);
            remaining = new_remaining;
        }
        else if let Ok((new_remaining, node)) = ctx_text(remaining) {
            nodes.push(node);
            remaining = new_remaining;
        }
        else {
            return Err(nom::Err::Error(nom::error::Error::new(remaining, nom::error::ErrorKind::Alt)));
        }
    }
    
    Ok((remaining, nodes))
}

fn parse_group_scoped_command(input: &str, command_name: String) -> IResult<&str, Node> {
    Ok((input, Node::Command(Command {
        name: command_name,
        style: CommandStyle::TexStyle,
        arg_style: ArgumentStyle::GroupScoped,
        options: Vec::new(),
        arguments: Vec::new(), 
    })))
}


