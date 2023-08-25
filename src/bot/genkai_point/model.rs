use {
    crate::bot::genkai_point::formula::GenkaiPointFormula,
    anyhow::{bail, Result},
    chrono::{DateTime, Duration, Utc},
    ordered_float::NotNan,
    serde::{Deserialize, Serialize},
};

#[allow(dead_code)]
pub(crate) const GENKAI_POINT_MIN: u64 = 0;
pub(crate) const GENKAI_POINT_MAX: u64 = 10;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct UserStat {
    pub(crate) user_id: u64,
    pub(crate) genkai_point: u64,
    pub(crate) total_vc_duration: Duration,
    pub(crate) efficiency: NotNan<f64>,
    pub(crate) last_activity_at: DateTime<Utc>,
}

impl UserStat {
    pub(crate) fn from_sessions(
        sessions: &[Session],
        formula: &impl GenkaiPointFormula,
    ) -> Result<Option<UserStat>> {
        if sessions.is_empty() {
            return Ok(None);
        }

        let user_id = sessions[0].user_id;
        let mut genkai_point = 0;
        let mut total_vc_duration = Duration::seconds(0);
        let mut last_activity_at = sessions[0].left_at();

        for session in sessions {
            if user_id != session.user_id {
                bail!("list contains different user's session");
            }

            genkai_point += formula.calc(session);

            last_activity_at = last_activity_at.max(session.left_at());

            // chrono::Duration has no AddAssign implementation.
            total_vc_duration = total_vc_duration + session.duration();
        }

        let mut efficiency = (genkai_point as f64 / GENKAI_POINT_MAX as f64)
            / (total_vc_duration.num_minutes() as f64 / 60.0);

        if efficiency.is_nan() {
            efficiency = 0.;
        }

        Ok(Some(UserStat {
            user_id,
            genkai_point,
            total_vc_duration,
            last_activity_at,
            // panic safety: already asserted !effieicncy.is_nan()
            efficiency: NotNan::new(efficiency).unwrap(),
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Session {
    pub(crate) user_id: u64,
    pub(crate) joined_at: DateTime<Utc>,
    pub(crate) left_at: Option<DateTime<Utc>>,
}

impl Session {
    pub(crate) fn duration(&self) -> Duration {
        self.left_at.unwrap_or_else(Utc::now) - self.joined_at
    }

    pub(crate) fn left_at(&self) -> DateTime<Utc> {
        self.left_at.unwrap_or_else(Utc::now)
    }
}

#[test]
fn stat_test() {
    use crate::bot::genkai_point::{datetime, formula::v1::FormulaV1};

    let test1 = UserStat::from_sessions(
        &[
            Session {
                user_id: 0,
                joined_at: datetime!(2021/3/1 00:00:00),
                left_at: Some(datetime!(2021/3/1 1:30:00)),
            },
            Session {
                user_id: 0,
                joined_at: datetime!(2021/3/2 00:00:00),
                left_at: Some(datetime!(2021/3/2 1:30:00)),
            },
        ],
        &FormulaV1,
    );

    let expected = UserStat {
        user_id: 0,
        genkai_point: 16,
        total_vc_duration: Duration::hours(3),
        last_activity_at: datetime!(2021/3/2 1:30:00),
        efficiency: NotNan::new(16.0 / 30.0).unwrap(),
    };

    assert_eq!(test1.unwrap().unwrap(), expected);

    assert!(UserStat::from_sessions(&[], &FormulaV1).unwrap().is_none());

    let test_conflicting_user_id = UserStat::from_sessions(
        &[
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
        ],
        &FormulaV1,
    );

    assert!(test_conflicting_user_id.is_err());
}
