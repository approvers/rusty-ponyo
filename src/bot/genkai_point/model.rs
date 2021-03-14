use {
    anyhow::{bail, Result},
    chrono::{DateTime, Duration, Timelike, Utc},
    chrono_tz::Asia::Tokyo,
    serde::Serialize,
};

pub(super) enum Command {
    Stats { user_id: u64 },
    JoinedVc { user_id: u64 },
    LeftVc { user_id: u64 },
}

#[derive(Clone, Serialize)]
pub(crate) struct Session {
    pub(crate) user_id: u64,
    pub(crate) joined_at: DateTime<Utc>,
    pub(crate) left_at: Option<DateTime<Utc>>,
}

impl Session {
    pub(crate) fn calc_point(&self) -> Result<u64> {
        if self.left_at.is_none() {
            bail!("this session is not finished yet.");
        }

        let joined_at = self.joined_at.with_timezone(&Tokyo);
        let left_at = self.left_at.unwrap();

        Ok((1..)
            .map(|x| joined_at + Duration::hours(x))
            .take_while(|x| *x <= left_at)
            .map(|x| x.hour())
            .map(hour_to_point)
            .sum())
    }

    pub(crate) fn duration(&self) -> Result<Duration> {
        if self.left_at.is_none() {
            bail!("this session is not finished yet.");
        }

        Ok(self.left_at.unwrap() - self.joined_at)
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
    use chrono::TimeZone;

    macro_rules! session_test {
        (
        from $y1:literal/$M1:literal/$d1:literal $h1:literal:$m1:literal:$s1:literal
        to $y2:literal/$M2:literal/$d2:literal $h2:literal:$m2:literal:$s2:literal
        gives $point:literal point
    ) => {{
            let session = Session {
                user_id: 0,
                joined_at: Tokyo
                    .ymd($y1, $M1, $d1)
                    .and_hms($h1, $m1, $s1)
                    .with_timezone(&Utc),
                left_at: Some(
                    Tokyo
                        .ymd($y2, $M2, $d2)
                        .and_hms($h2, $m2, $s2)
                        .with_timezone(&Utc),
                ),
            };
            assert_eq!(session.calc_point().unwrap(), $point);
        }};
    }

    session_test!(from 2021/3/1 23:39:00 to 2021/3/2 00:00:00 gives 0 point);
    session_test!(from 2021/3/1 23:39:00 to 2021/3/2 00:40:00 gives 7 point);
    session_test!(from 2021/3/1 00:00:00 to 2021/3/2 00:00:00 gives 76 point);
    session_test!(from 2021/3/1 00:10:00 to 2021/3/1 00:20:00 gives 0 point);
}
