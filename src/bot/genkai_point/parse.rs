// FIXME: it is better to use a parser library to implement this hell.

#[derive(Debug, PartialEq, Eq)]
pub(super) enum ParseError<'a> {
    /// failed to parse userid of argument of show command
    ShowUserId,
    /// unknown subcommand of ranking command
    RankingUnknownSubCommand,
    /// unknown option
    UnknownOption { unknowns: Vec<&'a str> },
    /// missing or invalid token after option
    MissingOrInvalidTokenAfterOption,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum RankBy {
    Point,
    Duration,
    Efficiency,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Command {
    Ranking {
        by: Option<RankBy>,
        invert_specified: bool,
    },
    Show {
        user_id: Option<u64>,
    },
    Graph {
        n: Option<u8>,
    },
    Help,

    // internal command variants
    Unspecified,
    Unknown,
}

pub(super) fn parse(msg: &str) -> Option<Result<Command, ParseError<'_>>> {
    use Command::*;

    let mut tokens = msg.split_ascii_whitespace().peekable();

    if let Some(&"限界ポイント") = tokens.peek() {
        let _ = tokens.next();
        return Some(parse_show(tokens));
    }

    // check for prefix first.
    // #![feature(let_else)]
    const PREFIX: &str = "g!point";
    if !matches!(tokens.next(), Some(PREFIX)) {
        return None;
    }

    // then subcommand comes
    Some(match tokens.next() {
        Some("help") => Ok(Help),
        None => Ok(Unspecified),
        Some("show") => parse_show(tokens),
        Some("ranking") => parse_ranking(tokens),
        Some("graph") => parse_graph(tokens),
        Some(_) => Ok(Unknown),
    })
}

// g!point graph
// Options:
//   -n <number>
fn parse_graph<'i>(mut tokens: impl Iterator<Item = &'i str>) -> Result<Command, ParseError<'i>> {
    let possibly_n = tokens.next();

    match possibly_n {
        Some("-n") => {}
        Some(s) => return Err(ParseError::UnknownOption { unknowns: vec![s] }),
        None => return Ok(Command::Graph { n: None }),
    }

    let possibly_n = tokens.next();
    match possibly_n.map(|x| x.parse()) {
        Some(Ok(n)) => Ok(Command::Graph { n: Some(n) }),
        Some(Err(_)) | None => Err(ParseError::MissingOrInvalidTokenAfterOption),
    }
}

// g!point show (user_id)?
fn parse_show<'i>(mut tokens: impl Iterator<Item = &'i str>) -> Result<Command, ParseError<'i>> {
    match tokens.next().map(|x| x.parse()) {
        Some(Ok(user_id)) => Ok(Command::Show {
            user_id: Some(user_id),
        }),

        None => Ok(Command::Show { user_id: None }),

        Some(Err(_)) => Err(ParseError::ShowUserId),
    }
}

// g!point ranking (point|efficiency|duration|<Options>)?
// Options:
//   -i, --invert

fn parse_ranking<'i>(tokens: impl Iterator<Item = &'i str>) -> Result<Command, ParseError<'i>> {
    let mut by = None;
    let mut invert_specified = false;
    let mut unknowns = vec![];

    for token in tokens {
        match token {
            "duration" => by = Some(RankBy::Duration),
            "efficiency" => by = Some(RankBy::Efficiency),
            "point" => by = Some(RankBy::Point),

            "-i" | "--invert" => invert_specified = true,

            p if p.starts_with('-') => unknowns.push(p),
            _ => return Err(ParseError::RankingUnknownSubCommand),
        };
    }

    if !unknowns.is_empty() {
        return Err(ParseError::UnknownOption { unknowns });
    }

    Ok(Command::Ranking {
        invert_specified,
        by,
    })
}

#[test]
fn parse_test() {
    use Command::*;
    use ParseError::*;
    use RankBy::*;

    assert_eq!(parse(""), None);
    assert_eq!(parse("g!alias"), None);

    assert_eq!(parse("g!point"), Some(Ok(Unspecified)));
    assert_eq!(parse("g!point hoge"), Some(Ok(Unknown)));

    assert_eq!(parse("g!point help"), Some(Ok(Help)));
    assert_eq!(parse("g!point     help"), Some(Ok(Help)));

    let r = |u| Some(Ok(Show { user_id: u }));
    assert_eq!(parse("限界ポイント"), r(None));
    assert_eq!(parse("g!point show"), r(None));
    assert_eq!(parse("限界ポイント 1234"), r(Some(1234)));
    assert_eq!(parse("g!point show 1234"), r(Some(1234)));
    assert_eq!(
        parse("g!point show hoge"),
        Some(Err(ParseError::ShowUserId))
    );

    let r = |by, in_| {
        Some(Ok(Command::Ranking {
            by,
            invert_specified: in_,
        }))
    };

    assert_eq!(parse("g!point ranking"), r(None, false));
    assert_eq!(parse("g!point ranking -i"), r(None, true));
    assert_eq!(parse("g!point ranking --invert"), r(None, true));
    assert_eq!(parse("g!point ranking -i -i"), r(None, true));

    // TODO: should we assume -ii as -i -i?
    assert_eq!(
        parse("g!point ranking -ii"),
        Some(Err(UnknownOption {
            unknowns: vec!["-ii"]
        }))
    );

    assert_eq!(parse("g!point ranking   point"), r(Some(Point), false));
    assert_eq!(parse("g!point ranking -i point"), r(Some(Point), true));
    assert_eq!(
        parse("g!point ranking point --invert"),
        r(Some(Point), true)
    );
    assert_eq!(parse("g!point ranking duration"), r(Some(Duration), false));
    assert_eq!(
        parse("g!point ranking efficiency"),
        r(Some(Efficiency), false)
    );
    assert_eq!(
        parse("g!point ranking efficiency -i"),
        r(Some(Efficiency), true)
    );

    assert_eq!(parse("g!point graph"), Some(Ok(Graph { n: None })));
    assert_eq!(
        parse("g!point graph -n 10"),
        Some(Ok(Graph { n: Some(10) }))
    );
    assert_eq!(
        parse("g!point graph -n"),
        Some(Err(MissingOrInvalidTokenAfterOption))
    );
    assert_eq!(
        parse("g!point graph -n foo"),
        Some(Err(MissingOrInvalidTokenAfterOption))
    );
}
