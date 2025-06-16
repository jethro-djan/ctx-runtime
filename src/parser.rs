use nom::{
    IResult, Parser,
    branch::alt,
    bytes::complete::{tag, take_until, take_while1},
    character::complete::{alpha1, char, not_line_ending},
    combinator::{map, opt, verify},
    multi::{many_till, many0},
    sequence::{delimited, preceded, terminated},
};
use nom_locate::LocatedSpan;
use std::collections::HashMap;

use crate::ast::{ArgumentStyle, CommandStyle, ConTeXtNode, SourceSpan};

type Span<'a> = LocatedSpan<&'a str>;

pub fn parse_document(input: &str) -> Result<ConTeXtNode, nom::Err<nom::error::Error<Span<'_>>>> {
    let span = Span::new(input);
    match parse_document_span(span) {
        Ok((_, node)) => Ok(node),
        Err(e) => Err(e),
    }
}

fn parse_document_span(input: Span) -> IResult<Span, ConTeXtNode> {
    if let Ok((remaining, _)) = tag::<_, _, nom::error::Error<Span>>("\\startdocument").parse(input) {
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
                    span: input.into(),
                }],
            },
        ));
    }

    let (input, preamble) = match alt((
        preceded(
            take_until::<_, _, nom::error::Error<Span>>("\\starttext"),
            tag("\\starttext"),
        ),
        preceded(
            take_until::<_, _, nom::error::Error<Span>>("\\startdocument"),
            tag("\\startdocument"),
        ),
    ))
    .parse(input)
    {
        Ok((remaining, starter)) => {
            let preamble_len = input.fragment().len() - remaining.fragment().len() - starter.fragment().len();
            let preamble_text = &input.fragment()[..preamble_len];
            let body_text = &input.fragment()[preamble_len + starter.fragment().len()..];
            
            (
                Span::new(body_text),
                preamble_text,
            )
        }
        Err(_) => (input, ""),
    };

    let end_marker = if input.fragment().contains("\\startdocument") {
        "\\stopdocument"
    } else {
        "\\stoptext"
    };

    let (input, body) = parse_until(input, end_marker)?;
    let (_, preamble_nodes) = many0(parse_node).parse(Span::new(preamble))?;
    let (_, body_nodes) = many0(parse_node).parse(body)?;

    Ok((
        input,
        ConTeXtNode::Document {
            preamble: preamble_nodes,
            body: body_nodes,
        },
    ))
}

fn parse_until<'a>(input: Span<'a>, end_marker: &str) -> IResult<Span<'a>, Span<'a>> {
    let end_bytes = end_marker.as_bytes();
    let bytes = input.fragment().as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        if bytes[i..].starts_with(end_bytes) {
            let content = &input.fragment()[..i];
            let remaining_text = &input.fragment()[i + end_bytes.len()..];
            return Ok((
                Span::new(remaining_text),
                Span::new(content)
            ));
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

pub fn parse_node(input: Span) -> IResult<Span, ConTeXtNode> {
    alt((parse_comment, parse_startstop, parse_command, parse_text)).parse(input)
}

pub fn parse_command(input: Span) -> IResult<Span, ConTeXtNode> {
    let start_span = input;
    let (input, _) = char('\\').parse(input)?;
    let (input, name) = alpha1.parse(input)?;

    if let Ok((remaining, (arg, opt))) = parse_context_style_args(input) {
        if !remaining.fragment().starts_with('{') {
            let mut options = HashMap::new();
            if let Some(opt) = opt {
                options.extend(opt);
            }
            return Ok((
                remaining,
                ConTeXtNode::Command {
                    name: name.fragment().to_string(),
                    style: CommandStyle::ContextStyle,
                    arg_style: ArgumentStyle::Explicit,
                    options,
                    arguments: vec![ConTeXtNode::Text {
                        content: arg.to_string(),
                        span: start_span.into(),
                    }],
                    span: start_span.into(),
                },
            ));
        }
    }

    let (style, arg_style) = match name.fragment() {
        &"item" => (CommandStyle::TexStyle, ArgumentStyle::LineEnding),
        &"em" | &"bf" | &"it" | &"tt" | &"rm" | &"sf" | &"sc" | &"sl" => {
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
            name: name.fragment().to_string(),
            style,
            arg_style,
            options: options.unwrap_or_default(),
            arguments,
            span: start_span.into(),
        },
    ))
}

pub fn parse_startstop(input: Span) -> IResult<Span, ConTeXtNode> {
    let start_span = input;
    let (input, _) = tag("\\start").parse(input)?;
    let (input, name) = alpha1.parse(input)?;
    let (input, options) = opt(parse_options).parse(input)?;

    let stop_tag = format!("\\stop{}", name.fragment());
    let (input, (content, _)) = many_till(parse_node, tag(&*stop_tag)).parse(input)?;

    Ok((
        input,
        ConTeXtNode::StartStop {
            name: name.fragment().to_string(),
            options: options.unwrap_or_default(),
            content,
            span: start_span.into(),
        },
    ))
}

pub fn parse_comment(input: Span) -> IResult<Span, ConTeXtNode> {
    let start_span = input;
    let (input, comment) = preceded(char('%'), not_line_ending).parse(input)?;

    Ok((
        input,
        ConTeXtNode::Comment {
            content: comment.fragment().trim().to_string(),
            span: start_span.into(),
        },
    ))
}

pub fn parse_text(input: Span) -> IResult<Span, ConTeXtNode> {
    let start_span = input;
    let (input, text) = verify(
        take_while1(|c| c != '\\' && c != '%' && c != '{' && c != '}'),
        |s: &Span| !s.fragment().is_empty(),
    ).parse(input)?;

    Ok((
        input,
        ConTeXtNode::Text {
            content: text.fragment().to_string(),
            span: start_span.into(),
        },
    ))
}

fn parse_context_style_args(input: Span<'_>) -> IResult<Span<'_>, (&str, Option<HashMap<String, String>>)> {
    let (input, arg) = delimited(char('['), take_until("]"), char(']')).parse(input)?;
    let (input, options) = opt(parse_options).parse(input)?;
    Ok((input, (arg.fragment(), options)))
}

fn parse_explicit_args(input: Span) -> IResult<Span, Vec<ConTeXtNode>> {
    opt(delimited(
        char('{'),
        many0(alt((
            parse_text,
            parse_comment,
            parse_command,
            parse_startstop,
        ))),
        char('}'),
    )).parse(input)
    .map(|(i, v)| (i, v.unwrap_or_default()))
}

fn parse_line_ending_args(input: Span) -> IResult<Span, Vec<ConTeXtNode>> {
    let start_span = input;
    let (input, text) = terminated(take_until("\n"), char('\n')).parse(input)?;

    Ok((
        input,
        vec![ConTeXtNode::Text {
            content: text.fragment().trim().to_string(),
            span: start_span.into(),
        }],
    ))
}

fn parse_group_scoped_args(input: Span) -> IResult<Span, Vec<ConTeXtNode>> {
    delimited(
        char('{'),
        many0(alt((parse_text, parse_comment, parse_command))),
        char('}'),
    ).parse(input)
}

pub fn parse_options(input: Span) -> IResult<Span, HashMap<String, String>> {
    delimited(
        char('['),
        map(take_until("]"), |s: Span| {
            s.fragment().split(',').filter_map(|pair| {
                let mut kv = pair.splitn(2, '=');
                Some((
                    kv.next()?.trim().to_string(),
                    kv.next().unwrap_or("true").trim().to_string(),
                ))
            })
            .collect()
        }),
        char(']'),
    ).parse(input)
}
