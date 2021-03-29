use {
    anyhow::{bail, Result},
    chrono::{DateTime, Duration, Timelike, Utc},
    chrono_tz::Asia::Tokyo,
    serde::Serialize,
};

#[allow(dead_code)]
pub(crate) const GENKAI_POINT_MIN: u64 = 0;
pub(crate) const GENKAI_POINT_MAX: u64 = 10;

#[derive(Debug, PartialEq)]
pub(crate) struct UserStat {
    pub(crate) user_id: u64,
    pub(crate) genkai_point: u64,
    pub(crate) total_vc_duration: Duration,
    pub(crate) efficiency: f64,
}

impl UserStat {
    pub(crate) fn from_sessions(sessions: &[Session]) -> Result<Option<UserStat>> {
        if sessions.is_empty() {
            return Ok(None);
        }

        let mut result = UserStat {
            user_id: sessions[0].user_id,
            genkai_point: 0,
            total_vc_duration: Duration::seconds(0),
            efficiency: 0.0,
        };

        for session in sessions {
            if result.user_id != session.user_id {
                bail!("list contains different user's session");
            }

            result.genkai_point += session.calc_point();
            result.total_vc_duration = result.total_vc_duration + session.duration();
        }

        result.efficiency = (result.genkai_point as f64 / GENKAI_POINT_MAX as f64)
            / (result.total_vc_duration.num_minutes() as f64 / 60.0);

        if result.efficiency.is_nan {
            result.efficiency = 0.;
        }

        Ok(Some(result))
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct Session {
    pub(crate) user_id: u64,
    pub(crate) joined_at: DateTime<Utc>,
    pub(crate) left_at: Option<DateTime<Utc>>,
}

impl Session {
    pub(crate) fn calc_point(&self) -> u64 {
        let joined_at = self.joined_at.with_timezone(&Tokyo);
        let left_at = self.left_at.unwrap_or_else(Utc::now);

        (1..)
            .map(|x| joined_at + Duration::hours(x))
            .take_while(|x| *x <= left_at)
            .map(|x| x.hour())
            .map(hour_to_point)
            .sum()
    }

    pub(crate) fn duration(&self) -> Duration {
        self.left_at.unwrap_or_else(Utc::now) - self.joined_at
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

#[cfg(test)]
macro_rules! datetime {
    ($y1:literal/$M1:literal/$d1:literal $h1:literal:$m1:literal:$s1:literal) => {
        Tokyo
            .ymd($y1, $M1, $d1)
            .and_hms($h1, $m1, $s1)
            .with_timezone(&Utc)
    };
}

#[test]
fn session_test() {
    use chrono::TimeZone;

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
            assert_eq!(session.calc_point(), $point);
        }};
    }

    session_test!(from (datetime!(2021/3/1 23:39:00)) to (datetime!(2021/3/2 00:00:00)) gives 0 point);
    session_test!(from (datetime!(2021/3/1 23:39:00)) to (datetime!(2021/3/2 00:40:00)) gives 7 point);
    session_test!(from (datetime!(2021/3/1 00:00:00)) to (datetime!(2021/3/2 00:00:00)) gives 76 point);
    session_test!(from (datetime!(2021/3/1 00:10:00)) to (datetime!(2021/3/1 00:20:00)) gives 0 point);
}

#[test]
fn point_min_max_test() {
    assert!(GENKAI_POINT_MIN < GENKAI_POINT_MAX);

    for point in (0..=23).map(hour_to_point) {
        assert!(GENKAI_POINT_MIN <= point && point <= GENKAI_POINT_MAX);
    }
}

#[test]
fn stat_test() {
    use chrono::TimeZone;

    let test1 = UserStat::from_sessions(&[
        Session {
            user_id: 0,
            joined_at: datetime!(2021/3/1 00:00:00),
            left_at: Some(datetime!(2021/3/1 01:30:00)),
        },
        Session {
            user_id: 0,
            joined_at: datetime!(2021/3/2 00:00:00),
            left_at: Some(datetime!(2021/3/2 01:30:00)),
        },
    ]);

    let expected = UserStat {
        user_id: 0,
        genkai_point: 16,
        total_vc_duration: Duration::hours(3),
        efficiency: 16.0 / 30.0,
    };

    assert_eq!(test1.unwrap().unwrap(), expected);

    assert!(UserStat::from_sessions(&[]).unwrap().is_none());

    let test_conflicting_user_id = UserStat::from_sessions(&[
        Session {
            user_id: 1,
            joined_at: Utc::now(),
            left_at: Some(Utc::now()),
        },
        Session {
            user_id: 2,
            joined_at: Utc::now(),
            left_at: Some(Utc::now()),
        },
    ]);

    assert!(test_conflicting_user_id.is_err());
}
