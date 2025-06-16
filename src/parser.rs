use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{alpha1, char, not_line_ending},
    combinator::{map, opt, verify},
    multi::{many_till, many0},
    sequence::{delimited, preceded, terminated},
};
use std::collections::HashMap;

use crate::ast::{ArgumentStyle, CommandStyle, ConTeXtNode, SourceSpan};

pub fn parse_document(input: &str) -> IResult<&str, ConTeXtNode> {
    if let Ok((remaining, _)) = tag::<_, _, nom::error::Error<&str>>("\\startdocument")(input) {
        let (remaining, content) = parse_until(remaining, "\\stopdocument")?;
        let (_, nodes) = many0(parse_node).parse(content)?;

        return Ok((
            remaining,
            ConTeXtNode::Document {
                preamble: Vec::new(),
                body: vec![ConTeXtNode::StartStop {
                    name: "document".to_string(),
                    options: HashMap::new(),
                    content: nodes,
                    span: SourceSpan::new(0, input.len() - remaining.len()),
                }],
            },
        ));
    }

    let (input, preamble) = match alt((
        preceded(
            take_until::<_, _, nom::error::Error<&str>>("\\starttext"),
            tag("\\starttext"),
        ),
        preceded(
            take_until::<_, _, nom::error::Error<&str>>("\\startdocument"),
            tag("\\startdocument"),
        ),
    ))
    .parse(input)
    {
        Ok((remaining, starter)) => {
            let preamble_len = input.len() - remaining.len() - starter.len();
            (
                &input[preamble_len + starter.len()..],
                &input[..preamble_len],
            )
        }
        Err(_) => (input, ""),
    };

    let end_marker = if input.contains("\\startdocument") {
        "\\stopdocument"
    } else {
        "\\stoptext"
    };

    let (input, body) = parse_until(input, end_marker)?;
    let (_, preamble_nodes) = many0(parse_node).parse(preamble)?;
    let (_, body_nodes) = many0(parse_node).parse(body)?;

    Ok((
        input,
        ConTeXtNode::Document {
            preamble: preamble_nodes,
            body: body_nodes,
        },
    ))
}

fn parse_until<'a>(input: &'a str, end_marker: &str) -> IResult<&'a str, &'a str> {
    let end_bytes = end_marker.as_bytes();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(end_bytes) {
            return Ok((&input[i + end_bytes.len()..], &input[..i]));
        }
        if bytes[i..].starts_with(b"\\start") {
            if let Some(end) = find_matching_stop(&bytes[i..]) {
                i += end;
                continue;
            }
        }
        i += 1;
    }
    Err(nom::Err::Error(nom::error::Error::new(
        input,
        nom::error::ErrorKind::Tag,
    )))
}

fn find_matching_stop(input: &[u8]) -> Option<usize> {
    let mut depth = 1;
    let mut pos = 6; // Skip past \start

    while pos < input.len() {
        if input[pos..].starts_with(b"\\start") {
            depth += 1;
            pos += 6;
        } else if input[pos..].starts_with(b"\\stop") && depth > 0 {
            depth -= 1;
            if depth == 0 {
                return Some(pos + 5); // Return position after \stop...
            }
            pos += 5;
        } else {
            pos += 1;
        }
    }
    None
}

pub fn parse_node(input: &str) -> IResult<&str, ConTeXtNode> {
    alt((parse_comment, parse_startstop, parse_command, parse_text)).parse(input)
}

pub fn parse_command(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, _) = char('\\')(input)?;
    let (input, name) = alpha1(input)?;

    if let Ok((remaining, (arg, opt))) = parse_context_style_args(input) {
        if !remaining.starts_with('{') {
            let mut options = HashMap::new();
            if let Some(opt) = opt {
                options.extend(opt);
            }
            return Ok((
                remaining,
                ConTeXtNode::Command {
                    name: name.to_string(),
                    style: CommandStyle::ContextStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options,
                    arguments: vec![ConTeXtNode::Text {
                        content: arg.to_string(),
                        span: dummy_span(),
                    }],
                    span: dummy_span(),
                },
            ));
        }
    }

    let (style, arg_style) = match name {
        "item" => (CommandStyle::TexStyle, ArgumentStyle::LineEnding),
        "em" | "bf" | "it" | "tt" | "rm" | "sf" | "sc" | "sl" => {
            (CommandStyle::TexStyle, ArgumentStyle::GroupScoped)
        }
        _ => (CommandStyle::TexStyle, ArgumentStyle::Explicit),
    };

    let (input, options) = opt(parse_options).parse(input)?;
    let (input, arguments) = match arg_style {
        ArgumentStyle::LineEnding => parse_line_ending_args(input)?,
        ArgumentStyle::GroupScoped => parse_group_scoped_args(input)?,
        ArgumentStyle::Explicit => parse_explicit_args(input)?,
    };

    Ok((
        input,
        ConTeXtNode::Command {
            name: name.to_string(),
            style,
            arg_style,
            options: options.unwrap_or_default(),
            arguments,
            span: dummy_span(),
        },
    ))
}

pub fn parse_startstop(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, _) = tag("\\start")(input)?;
    let (input, name) = alpha1(input)?;
    let (input, options) = opt(parse_options).parse(input)?;

    let stop_tag = format!("\\stop{}", name);
    let (input, (content, _)) = many_till(parse_node, tag(&*stop_tag)).parse(input)?;

    Ok((
        input,
        ConTeXtNode::StartStop {
            name: name.to_string(),
            options: options.unwrap_or_default(),
            content,
            span: dummy_span(),
        },
    ))
}

pub fn parse_comment(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, comment) = preceded(char('%'), not_line_ending).parse(input)?;

    Ok((
        input,
        ConTeXtNode::Comment {
            content: comment.trim().to_string(),
            span: dummy_span(),
        },
    ))
}

pub fn parse_text(input: &str) -> IResult<&str, ConTeXtNode> {
    let (input, text) = verify(
        take_while1(|c| c != '\\' && c != '%' && c != '{' && c != '}'),
        |s: &str| !s.is_empty(),
    )
    .parse(input)?;

    Ok((
        input,
        ConTeXtNode::Text {
            content: text.to_string(),
            span: dummy_span(),
        },
    ))
}

fn parse_context_style_args(input: &str) -> IResult<&str, (&str, Option<HashMap<String, String>>)> {
    let (input, arg) = delimited(char('['), take_until("]"), char(']')).parse(input)?;

    let (input, options) = opt(parse_options).parse(input)?;

    Ok((input, (arg, options)))
}

fn parse_explicit_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    opt(delimited(
        char('{'),
        many0(alt((
            parse_text,
            parse_comment,
            parse_command,
            parse_startstop,
        ))),
        char('}'),
    ))
    .parse(input)
    .map(|(i, v)| (i, v.unwrap_or_default()))
}

fn parse_line_ending_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    let (input, text) = terminated(take_until("\n"), char('\n')).parse(input)?;

    Ok((
        input,
        vec![ConTeXtNode::Text {
            content: text.trim().to_string(),
            span: dummy_span(),
        }],
    ))
}

fn parse_group_scoped_args(input: &str) -> IResult<&str, Vec<ConTeXtNode>> {
    delimited(
        char('{'),
        |content| many0(alt((parse_text, parse_comment, parse_command))).parse(content),
        char('}'),
    )
    .parse(input)
}

pub fn parse_options(input: &str) -> IResult<&str, HashMap<String, String>> {
    delimited(
        char('['),
        map(take_until("]"), |s: &str| {
            {
                s.split(',').filter_map(|pair| {
                    let mut kv = pair.splitn(2, '=');
                    Some((
                        kv.next()?.trim().to_string(),
                        kv.next().unwrap_or("true").trim().to_string(),
                    ))
                })
            }
            .collect()
        }),
        char(']'),
    )
    .parse(input)
}

pub fn dummy_span() -> SourceSpan {
    SourceSpan { start: 0, end: 0 }
}
