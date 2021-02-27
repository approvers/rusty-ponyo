use {
    super::PREFIX,
    std::{ops::Range, str::Chars},
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Sentence<'a> {
    pub(super) prefix: &'a str,
    pub(super) sub_command: Option<&'a str>,
    pub(super) args: Vec<String>,
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
        buffer: String,
        was_before_backslash: bool,
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
                        buffer: String::new(),
                        was_before_backslash: false,
                    }
                }

                c if c.is_whitespace() => {}

                c => {
                    return Err(format_error(
                        text,
                        chars_index..chars_index,
                        &format!(r#"Expected '"', but found '{}'"#, c),
                        Some("You probably forgot to quote string argument."),
                    ));
                }
            },

            InString {
                mut buffer,
                was_before_backslash,
            } => {
                if was_before_backslash {
                    if ch == '"' {
                        buffer.push('"');
                        state = InString {
                            buffer,
                            was_before_backslash: false,
                        };

                        continue;
                    }

                    return Err(format_error(
                        text,
                        (chars_index - 1)..chars_index,
                        &format!("This escape sequence is not supported."),
                        None,
                    ));
                }

                if ch == '\\' {
                    state = InString {
                        buffer,
                        was_before_backslash: true,
                    };
                    continue;
                }

                if ch == '"' {
                    args.push(buffer);
                    state = BeforeString;
                    continue;
                }

                buffer.push(ch);
                state = InString {
                    buffer,
                    was_before_backslash,
                };
            }
        }
    }

    let index = text.len();

    match state {
        BeforeString | BeforeSubCommand | BeforePrefix => {}
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
                chars_index..chars_index,
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

fn format_error(origin: &str, pos: Range<isize>, error_msg: &str, hint: Option<&str>) -> String {
    let mut result = format!(
        "```\nParsing Error(at {}-{}): {}\n{}",
        pos.start, pos.end, error_msg, origin
    );

    // 全角文字ではマーカーがどうしてもずれるので表示しない
    if origin.chars().all(|x| x.is_ascii()) {
        let marker = Some(' ')
            .iter()
            .cycle()
            .take(pos.start as usize)
            .chain(
                Some('^')
                    .iter()
                    .cycle()
                    .take(pos.end as usize - pos.start as usize + 1),
            )
            .collect::<String>();

        result.push('\n');
        result.push_str(&marker);
    }

    if let Some(hint) = hint {
        const HINT_PREFIX: &str = "\nhint: ";
        result.push_str(HINT_PREFIX);
        result.push_str(hint);
    }

    result.push_str("\n```");

    result
}
