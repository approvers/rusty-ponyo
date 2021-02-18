use {
    super::PREFIX,
    std::{ops::Range, str::Chars},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Sentence<'a> {
    pub(super) prefix: &'a str,
    pub(super) sub_command: Option<&'a str>,
    pub(super) args: Vec<&'a str>,
}

enum ParseState {
    BeforePrefix,
    InPrefix {
        begin: usize,
        prefix_iter: Chars<'static>,
    },
    BeforeSubCommand,
    InSubCommand {
        begin: usize,
    },
    BeforeString,
    InString {
        begin: usize,
    },
}

pub(super) fn parse<'a>(text: &'a str) -> Result<Option<Sentence<'a>>, String> {
    use ParseState::*;

    let chars = text.char_indices().collect::<Vec<_>>();
    let mut chars_index: isize = -1;

    let mut prefix = None;
    let mut sub_command = None;
    let mut args = vec![];

    let mut state = BeforePrefix;

    loop {
        chars_index += 1;

        let (index, ch) = match chars.get(chars_index as usize) {
            None => break,
            Some(t) => *t,
        };

        match state {
            BeforePrefix => {
                if ch.is_whitespace() {
                    continue;
                }

                state = InPrefix {
                    begin: index,
                    prefix_iter: PREFIX.chars(),
                };

                chars_index -= 1;
            }

            InPrefix {
                ref mut prefix_iter,
                begin,
            } => {
                let reference = prefix_iter.next();

                if reference.is_none() && ch.is_whitespace() {
                    prefix = Some(&text[begin..index]);

                    state = BeforeSubCommand;
                    continue;
                }

                let reference = reference.unwrap();
                if reference != ch {
                    return Ok(None);
                }
            }

            BeforeSubCommand => {
                if ch.is_whitespace() {
                    continue;
                }

                state = InSubCommand { begin: index };
            }

            InSubCommand { begin } => {
                if ch.is_whitespace() {
                    sub_command = Some(&text[begin..index]);
                    state = BeforeString;
                }
            }

            BeforeString => match ch {
                '"' => {
                    state = InString {
                        begin: index + '"'.len_utf8(),
                    }
                }

                c if c.is_whitespace() => {}

                c => {
                    let index = chars_index as usize;
                    return Err(format_error(
                        text,
                        index..index,
                        &format!(r#"Expected '"', but found '{}'"#, c),
                        Some("You probably forgot to quote string argument."),
                    ));
                }
            },

            InString { begin } => {
                if ch == '"' {
                    args.push(&text[begin..index]);
                    state = BeforeString;
                }
            }
        }
    }

    let index = text.len();

    match state {
        BeforePrefix => return Ok(None),

        BeforeString | BeforeSubCommand => {}

        InPrefix {
            begin,
            mut prefix_iter,
        } => {
            if prefix_iter.next().is_none() {
                prefix = Some(&text[begin..index]);
            }
        }
        InSubCommand { begin } => sub_command = Some(&text[begin..index]),
        InString { .. } => {
            return Err(format_error(
                text,
                index..index,
                "Unexpected end of text while parsing String argument",
                Some("You probably forgot to put double quote at the end."),
            ));
        }
    }

    Ok(Some(Sentence {
        prefix: prefix.unwrap(),
        sub_command,
        args,
    }))
}

fn format_error(origin: &str, pos: Range<usize>, error_msg: &str, hint: Option<&str>) -> String {
    let marker = Some(' ')
        .iter()
        .cycle()
        .take(pos.start)
        .chain(Some('^').iter().cycle().take(pos.end - pos.start + 1))
        .collect::<String>();

    let mut result = format!("```\nParsing Error: {}\n{}\n{}", error_msg, origin, marker);

    if let Some(hint) = hint {
        const HINT_PREFIX: &str = "\nhint: ";
        result.push_str(HINT_PREFIX);
        result.push_str(hint);
    }

    result.push_str("\n```");

    result
}
