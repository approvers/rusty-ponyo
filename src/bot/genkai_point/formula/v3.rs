use {super::GenkaiPointFormula, crate::bot::genkai_point::model::Session, chrono_tz::Asia::Tokyo};

pub(crate) struct FormulaV3;

impl GenkaiPointFormula for FormulaV3 {
    fn name(&self) -> &'static str {
        "v3"
    }

    fn calc(&self, session: &Session) -> u64 {
        let jdt = session.joined_at.with_timezone(&Tokyo);
        let ldt = session.left_at().with_timezone(&Tokyo);

        const ONE_DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;
        let c = jdt.timestamp_millis() % ONE_DAY_MILLIS;
        let c = c as f64 / ONE_DAY_MILLIS as f64;

        let pts = (0..(ldt - jdt).num_minutes())
            .map(|t| t as f64 / 60.0)
            .map(|t| formula(c, t))
            .sum::<f64>();

        pts as u64
    }
}

// weight-duration graph (rendered on web): https://www.geogebra.org/graphing/vcczhr5s
fn formula(c: f64, t: f64) -> f64 {
    const PI: f64 = core::f64::consts::PI;

    (4.0 / PI)
        * (t / 5.0 + 1.0).log2()
        * ((-t / 5.0).atan() + PI / 2.0)
        * ((((t + 5.0 + c) / 12.0 * PI).sin() + 1.0) / 2.0).powi(2)
}

// これはテストではないのでテストではない
#[test]
fn session_test() {
    use crate::bot::genkai_point::datetime;

    macro_rules! session_test {
        ($tag:literal : $jdt:expr => $ldt:expr) => {{
            let session = Session {
                user_id: 0,
                joined_at: $jdt,
                left_at: Some($ldt),
            };

            println!(
                "{} : <{} [min]> {} <x{}>",
                $tag,
                ($ldt - $jdt).num_minutes(),
                FormulaV3.calc(&session),
                FormulaV3.calc(&session) as f64 / ($ldt - $jdt).num_minutes() as f64
            );
        }};
    }

    session_test!("very sh" : datetime!(2021/1/2 00:00:00) => datetime!(2021/1/2 01:00:00));
    session_test!("short  " : datetime!(2021/1/1 21:00:00) => datetime!(2021/1/2 23:00:00));
    session_test!("trend  " : datetime!(2021/1/1 20:00:00) => datetime!(2021/1/2 03:00:00));
    session_test!("daytime" : datetime!(2021/1/1 10:00:00) => datetime!(2021/1/2 20:00:00));
}
