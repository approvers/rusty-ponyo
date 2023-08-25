use {
    crate::bot::genkai_point::{formula::GenkaiPointFormula, model::Session},
    chrono::{DateTime, Duration, TimeZone, Timelike},
    chrono_tz::Asia::Tokyo,
};

pub(crate) struct FormulaV2;

impl GenkaiPointFormula for FormulaV2 {
    fn name(&self) -> &'static str {
        "v2"
    }

    fn calc(&self, session: &Session) -> u64 {
        let start = session.joined_at.with_timezone(&Tokyo);
        let end = session.left_at().with_timezone(&Tokyo);

        let mut start_cursor = start;

        let sub = |a: f64, b: f64, f: fn(f64) -> f64| f(b) - f(a);

        let mut res = 0.0;

        while end > start_cursor {
            let end_cursor;

            // v1 に用いた関数をそれぞれ積分したもの

            let d = match start_cursor.hour() {
                0..=2 => {
                    end_cursor = end.min(start_cursor.with_hms(3, 0, 0).unwrap());
                    sub(start_cursor.hour_f64(), end_cursor.hour_f64(), |x| {
                        (x.powi(2) / 2.0) + (7.0 * x)
                    })
                }
                3..=5 => {
                    end_cursor = end.min(start_cursor.with_hms(6, 0, 0).unwrap());
                    sub(start_cursor.hour_f64(), end_cursor.hour_f64(), |x| {
                        -(x.powi(2) / 2.0) + (13.0 * x)
                    })
                }
                6..=8 => {
                    end_cursor = end.min(start_cursor.with_hms(9, 0, 0).unwrap());
                    sub(start_cursor.hour_f64(), end_cursor.hour_f64(), |x| {
                        -x.powi(2) + (19.0 * x)
                    })
                }
                9 => {
                    end_cursor = end.min(start_cursor.with_hms(10, 0, 0).unwrap());
                    sub(start_cursor.hour_f64(), end_cursor.hour_f64(), |x| {
                        -(x.powi(2) / 2.0) + (10.0 * x)
                    })
                }
                10..=19 => {
                    end_cursor = end.min(start_cursor.with_hms(20, 0, 0).unwrap());
                    0.0
                }
                20 => {
                    end_cursor = end.min(start_cursor.with_hms(21, 0, 0).unwrap());
                    sub(start_cursor.hour_f64(), end_cursor.hour_f64(), |x| {
                        (x.powi(2) / 2.0) - (20.0 * x)
                    })
                }
                21..=23 => {
                    end_cursor =
                        end.min(start_cursor.with_hms(0, 0, 0).unwrap() + Duration::days(1));

                    let e = end_cursor.hour_f64();
                    let e = if e == 0.0 { 23.9999 } else { e };

                    sub(start_cursor.hour_f64(), e, |x| x.powi(2) - (41.0 * x))
                }
                x => unreachable!("hour {x} is not possible"),
            };

            res += d;
            start_cursor = end_cursor;
        }

        res.round() as _
    }
}

trait DateTimeExt<Tz: TimeZone> {
    fn with_hms(&self, hour: u32, minute: u32, second: u32) -> Option<DateTime<Tz>>;
    fn hour_f64(&self) -> f64;
}

impl<Tz: TimeZone> DateTimeExt<Tz> for DateTime<Tz> {
    fn with_hms(&self, hour: u32, minute: u32, second: u32) -> Option<DateTime<Tz>> {
        self.with_hour(hour)?
            .with_minute(minute)?
            .with_second(second)
    }

    fn hour_f64(&self) -> f64 {
        self.hour() as f64 + (self.minute() as f64 / 60.0) + (self.second() as f64 / 3600.0)
    }
}

#[test]
fn session_test() {
    use {crate::bot::genkai_point::datetime, pretty_assertions::assert_eq};

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
            assert_eq!(FormulaV2.calc(&session), $point);
        }};
    }

    session_test!(from (datetime!(2021/3/1 23:00:00)) to (datetime!(2021/3/2 00:00:00)) gives 6 point);
    session_test!(from (datetime!(2021/3/1 23:00:00)) to (datetime!(2021/3/2 00:40:00)) gives 11 point);
    session_test!(from (datetime!(2021/3/1 00:00:00)) to (datetime!(2021/3/2 00:00:00)) gives 76 point);
    session_test!(from (datetime!(2021/3/1 00:10:00)) to (datetime!(2021/3/1 00:20:00)) gives 1 point);
}
