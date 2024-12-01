use {
    crate::bot::genkai_point::{
        formula::{GenkaiPointFormula, GenkaiPointFormulaOutput},
        model::Session,
    },
    chrono::{Duration, Timelike, Utc},
    chrono_tz::Asia::Tokyo,
};

pub struct FormulaV1;

impl GenkaiPointFormula for FormulaV1 {
    fn name(&self) -> &'static str {
        "v1"
    }

    fn calc(&self, sessions: &[Session]) -> GenkaiPointFormulaOutput {
        let point = sessions
            .iter()
            .map(|session| {
                let joined_at = session.joined_at.with_timezone(&Tokyo);
                let left_at = session.left_at.unwrap_or_else(Utc::now);

                (1..)
                    .map(|x| joined_at + Duration::hours(x))
                    .take_while(|x| *x <= left_at)
                    .map(|x| x.hour())
                    .map(hour_to_point)
                    .sum::<u64>()
            })
            .sum();

        let total_hours = sessions
            .iter()
            .map(|s| s.left_at() - s.joined_at)
            .sum::<Duration>()
            .num_milliseconds() as f64
            / (60.0 * 60.0 * 1000.0);

        let efficiency = (point as f64 / 10.0) / total_hours;

        GenkaiPointFormulaOutput { point, efficiency }
    }
}

fn hour_to_point(hour: u32) -> u64 {
    // see also: https://imgur.com/a/1l3bujI
    match hour {
        0 => 7,
        1 => 8,
        2 => 9,
        3 => 10,
        4 => 9,
        5 => 8,
        6 => 7,
        7 => 5,
        8 => 3,
        9 => 1,
        n if (10..=20).contains(&n) => 0,
        21 => 1,
        22 => 3,
        23 => 5,
        _ => panic!("specified hour does not exist"),
    }
}

#[test]
fn session_test() {
    use crate::bot::genkai_point::datetime;

    macro_rules! session_test {
        (
            from ($d1:expr)
            to ($d2:expr)
            gives $point:literal point
        ) => {{
            let session = Session {
                user_id: 0,
                joined_at: $d1,
                left_at: Some($d2),
            };
            assert_eq!(FormulaV1.calc(&[session]).point, $point);
        }};
    }

    session_test!(from (datetime!(2021/3/1 23:39:00)) to (datetime!(2021/3/2 00:00:00)) gives 0 point);
    session_test!(from (datetime!(2021/3/1 23:39:00)) to (datetime!(2021/3/2 00:40:00)) gives 7 point);
    session_test!(from (datetime!(2021/3/1 00:00:00)) to (datetime!(2021/3/2 00:00:00)) gives 76 point);
    session_test!(from (datetime!(2021/3/1 00:10:00)) to (datetime!(2021/3/1 00:20:00)) gives 0 point);
}
