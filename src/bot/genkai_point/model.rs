use {
    crate::bot::genkai_point::formula::{GenkaiPointFormula, GenkaiPointFormulaOutput},
    anyhow::{Result, bail},
    chrono::{DateTime, Duration, Utc},
    ordered_float::NotNan,
    serde::{Deserialize, Serialize},
};

#[derive(Debug, PartialEq, Eq)]
pub struct UserStat {
    pub user_id: u64,
    pub genkai_point: u64,
    pub total_vc_duration: Duration,
    pub efficiency: NotNan<f64>,
    pub last_activity_at: DateTime<Utc>,
}

impl UserStat {
    pub fn from_sessions(
        sessions: &[Session],
        formula: &impl GenkaiPointFormula,
    ) -> Result<Option<UserStat>> {
        if sessions.is_empty() {
            return Ok(None);
        }

        let user_id = sessions[0].user_id;
        if sessions.iter().any(|s| s.user_id != user_id) {
            bail!("list contains different user's session");
        }

        let GenkaiPointFormulaOutput { point, efficiency } = formula.calc(sessions);
        let efficiency = if efficiency.is_nan() { 0.0 } else { efficiency };

        let total_vc_duration = sessions
            .iter()
            .fold(Duration::zero(), |acc, s| acc + s.duration());

        let last_activity_at = sessions
            .iter()
            .fold(sessions[0].joined_at, |acc, s| acc.max(s.left_at()));

        Ok(Some(UserStat {
            user_id,
            genkai_point: point,
            total_vc_duration,
            last_activity_at,
            // panic safety: already asserted !effieicncy.is_nan()
            efficiency: NotNan::new(efficiency).unwrap(),
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: u64,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
}

impl Session {
    pub fn duration(&self) -> Duration {
        self.left_at.unwrap_or_else(Utc::now) - self.joined_at
    }

    pub fn left_at(&self) -> DateTime<Utc> {
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
