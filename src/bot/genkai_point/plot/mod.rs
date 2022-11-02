use {
    crate::bot::{
        genkai_point::{model::Session, GenkaiPointDatabase},
        Context,
    },
    anyhow::{Context as _, Result},
    chrono::{Date, DateTime, Duration, Utc},
    chrono_tz::{Asia::Tokyo, Tz},
    std::{cmp::Reverse, collections::HashMap},
};

#[cfg(feature = "plot_matplotlib")]
pub(super) mod matplotlib;

#[cfg(feature = "plot_plotters")]
pub(super) mod plotters;

pub(super) trait Plotter {
    fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>>;
}

pub(super) async fn plot<P: Plotter + Send>(
    db: &impl GenkaiPointDatabase,
    ctx: &dyn Context,
    plotter: P,
    top: usize,
) -> Result<Option<Vec<u8>>> {
    let all_sessions = {
        let sess = db.get_all_sessions().await?;

        if sess.is_empty() {
            return Ok(None);
        }

        let mut sess = sess
            .into_iter()
            .map(TzAwareSession::from)
            .collect::<Vec<_>>();

        sess.sort_unstable_by_key(|x| x.joined_at);
        sess
    };

    let duration_progess_per_user = {
        let all_sessions_range = sessions_range(&all_sessions);

        let mut sess = HashMap::new();
        for session in all_sessions {
            sess.entry(session.user_id)
                .or_insert_with(Vec::new)
                .push(session);
        }

        let mut dur = Vec::with_capacity(sess.capacity());
        for (user_id, user_sessions) in sess {
            if let Some(mut progress) = sessions_to_duration_progress(&user_sessions) {
                align_duration_progresses(
                    &mut progress,
                    sessions_range(&user_sessions),
                    all_sessions_range,
                );
                dur.push((user_id, progress));
            }
        }

        dur.sort_unstable_by_key(|(_, x)| Reverse(*x.last().unwrap()));
        dur
    };

    let prottable_data = {
        let mut data = vec![];
        for (user_id, progress) in duration_progess_per_user.into_iter().take(top) {
            let user_name = ctx.get_user_name(user_id).await?;
            let progress = progress
                .into_iter()
                .map(|x| x.num_seconds() as f64 / (60 * 60) as f64)
                .collect();
            data.push((user_name, progress));
        }
        data
    };

    // FIXME: plotter.plot can take unacceptable time for tokio runtime maybe?
    //        use tokio::task::spawn_blocking to solve this problem.
    let image = plotter
        .plot(prottable_data)
        .context("failed to plot graph")?;

    Ok(Some(image))
}

struct TzAwareSession {
    user_id: u64,
    joined_at: DateTime<Tz>,
    left_at: DateTime<Tz>,
}

impl From<Session> for TzAwareSession {
    fn from(s: Session) -> TzAwareSession {
        let utc_to_jst = |d: DateTime<Utc>| (d + Duration::hours(9)).with_timezone(&Tokyo);

        TzAwareSession {
            user_id: s.user_id,
            joined_at: utc_to_jst(s.joined_at),
            left_at: utc_to_jst(s.left_at.unwrap_or_else(Utc::now)),
        }
    }
}

fn sessions_range(sessions: &[TzAwareSession]) -> (Date<Tz>, Date<Tz>) {
    let min = sessions.iter().map(|x| x.joined_at.date()).min().unwrap();
    let max = sessions.iter().map(|x| x.left_at.date()).max().unwrap();
    (min, max)
}

fn align_duration_progresses(
    target: &mut Vec<Duration>,
    target_range: (Date<Tz>, Date<Tz>),
    align_to: (Date<Tz>, Date<Tz>),
) {
    debug_assert!(align_to.0 <= target_range.0);
    debug_assert!(align_to.1 >= target_range.1);
    debug_assert!(!target.is_empty());

    // align beginning
    let mut beginning_aligned =
        vec![Duration::zero(); (target_range.0 - align_to.0).num_days() as usize];
    beginning_aligned.append(target);

    *target = beginning_aligned;

    let last = *target.last().unwrap();

    // align ending
    for _ in 0..(align_to.1 - target_range.1).num_days() {
        target.push(last);
    }

    debug_assert_eq!(
        target.len(),
        (align_to.1 - align_to.0).num_days() as usize + 1
    );
}

#[test]
fn test_align_duration_progress() {
    use chrono::TimeZone;

    let range = (Tokyo.ymd(2022, 1, 10), Tokyo.ymd(2022, 1, 15));
    let target_range = (Tokyo.ymd(2022, 1, 11), Tokyo.ymd(2022, 1, 13));
    let mut target = [3, 5, 10].into_iter().map(Duration::seconds).collect();

    align_duration_progresses(&mut target, target_range, range);

    assert_eq!(target, [0, 3, 5, 10, 10, 10].map(Duration::seconds));
}

fn sessions_to_duration_progress(tz_aware_sessions: &[TzAwareSession]) -> Option<Vec<Duration>> {
    debug_assert!(tz_aware_sessions
        .iter()
        .all(|x| x.user_id == tz_aware_sessions[0].user_id));

    // is_sorted is not stable yet.
    // debug_assert!(tz_aware_sessions.is_sorted_by_key(|x| x.joined_at));

    if tz_aware_sessions.len() < 2 {
        return None;
    }

    let begin_date = tz_aware_sessions.first().unwrap().joined_at.date();
    let end_date = tz_aware_sessions.last().unwrap().left_at.date();

    let result_len = (end_date - begin_date).num_days() as usize + 1;
    let mut result = vec![None; result_len];
    result[0] = Some(Duration::zero());

    for session in tz_aware_sessions {
        let mut cursor_begin = session.joined_at;
        let cursor_end = session.left_at;

        loop {
            let finish = cursor_begin.date() == cursor_end.date();

            let delta_this_day = {
                if finish {
                    cursor_end - cursor_begin
                } else {
                    let end_of_day = cursor_begin.date().and_hms(23, 59, 59);
                    end_of_day - cursor_begin
                }
            };

            let index = (cursor_begin.date() - begin_date).num_days() as usize;

            if result[index].is_none() {
                fill_from_last_some(&mut result, index);
            }

            // chrono::Duration has no AddAssign implementation.
            result[index] = Some(*result[index].as_ref().unwrap() + delta_this_day);

            if finish {
                break;
            }

            cursor_begin = cursor_begin.date().succ().and_hms(0, 0, 0);
        }
    }

    fill_from_last_some(&mut result, result_len - 1);

    Some(
        result
            .into_iter()
            .map(|x| x.expect("fill_from_last_some did not actually filled"))
            .collect(),
    )
}

// Suppose we have
// SSSSNNNNNNNNN
//         ~ index
// where S is Some, N is None.
//
// This function modifies list into like this:
// SSSSSSSSSNNNN
// ~~~~     ~~~~ untouched
//    ~ src
//         ~ index
//     ~~~~~ cloned from src
//
// If src element is not found, this function panics.
fn fill_from_last_some(list: &mut [Option<impl Clone>], index: usize) {
    let (src_index, src) = list
        .iter()
        .enumerate()
        .take(index + 1)
        .rev()
        .find(|(_, v)| v.is_some())
        .map(|(i, v)| (i, v.clone()))
        .expect("src element is not found");

    for i in (src_index + 1)..=index {
        list[i] = src.clone();
    }
}

#[test]
fn test_fill_from_last_some() {
    let mut list = vec![Some(0), Some(1), None, None, None];
    fill_from_last_some(&mut list, 3);
    assert_eq!(list, vec![Some(0), Some(1), Some(1), Some(1), None]);

    let mut list = vec![Some(0), None, None, None, None];
    fill_from_last_some(&mut list, 0);
    assert_eq!(list, vec![Some(0), None, None, None, None]);
}
