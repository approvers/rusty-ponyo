use {
    super::{GenkaiPointFormula, GenkaiPointFormulaOutput},
    crate::bot::genkai_point::model::Session,
    bigdecimal::{BigDecimal as Real, FromPrimitive as _, ToPrimitive as _},
    chrono_tz::Asia::Tokyo,
};

pub(crate) struct FormulaV3;

impl GenkaiPointFormula for FormulaV3 {
    fn name(&self) -> &'static str {
        "v3"
    }

    fn calc(&self, sessions: &[Session]) -> GenkaiPointFormulaOutput {
        let (now_points, max_points) = sessions
            .iter()
            .map(|s| {
                let jdt = s.joined_at.with_timezone(&Tokyo);
                let ldt = s.left_at().with_timezone(&Tokyo);

                (jdt, ldt)
            })
            .map(|(jdt, ldt)| {
                const ONE_HOUR_MILLIS: i64 = 60 * 60 * 1000;

                let c = Real::from(jdt.timestamp_millis() % (24 * ONE_HOUR_MILLIS))
                    / Real::from(ONE_HOUR_MILLIS);

                let t = Real::from((ldt - jdt).num_milliseconds()) / Real::from(ONE_HOUR_MILLIS);

                let n23 = Real::from(23);
                let n00 = Real::from(0);

                #[rustfmt::skip]
                let now_point = formula(  c.clone(), t.clone()) - formula(  c.clone(), n00.clone());
                let max_point = formula(n23.clone(), t.clone()) - formula(n23.clone(), n00.clone());

                (now_point, max_point)
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let now_point: Real = now_points.iter().sum::<Real>() * 10;
        let max_point: Real = max_points.iter().sum::<Real>() * 10;

        let point = now_point.round(0).to_u64().unwrap();
        let efficiency = (now_point / max_point).to_f64().unwrap();

        GenkaiPointFormulaOutput { point, efficiency }
    }
}

#[rustfmt::skip]
// rendered: https://www.geogebra.org/graphing/esdsm7rz
// used for integrate: https://www.integral-calculator.com
fn formula(c: Real, t: Real) -> Real {
    let pi = Real::from_f64(core::f64::consts::PI).unwrap();

    let sin = |n: Real| Real::from_f64(n.to_f64().unwrap().sin()).unwrap();
    let cos = |n: Real| Real::from_f64(n.to_f64().unwrap().cos()).unwrap();

    let exp  = |n: Real| n.exp();
    let pow2 = |n: Real| n.square();

    let pi2 = pow2(pi.clone());

    let pi2_36: Real = pi2.clone() + 36;
    let pi2_09: Real = pi2.clone() +  9;

    let pi2_36_2 = pow2(pi2_36.clone());
    let pi2_09_2 = pow2(pi2_09.clone());

    let tc5_pi_06: Real = ((t.clone() + c.clone() + 5) * pi.clone() /  6) % pi.double();
    let tc5_pi_12: Real = ((t.clone() + c.clone() + 5) * pi.clone() / 12) % pi.double();

    let ex0 =      pi.clone() * pi2_36_2.clone() * (pi2_09.clone() * t.clone() + 36                   ) * sin(tc5_pi_06.clone());
    let ex1 = -3 *              pi2_36_2.clone() * (pi2_09.clone() * t.clone() + 18  - 2 * pi2.clone()) * cos(tc5_pi_06.clone());
    let ex2 = 48 *              pi2_09_2.clone() * (pi2_36.clone() * t.clone() + 72  - 2 * pi2.clone()) * sin(tc5_pi_12.clone());
    let ex3 =  8 * pi.clone() * pi2_09_2.clone() * (pi2_36.clone() * t.clone() + 144                  ) * cos(tc5_pi_12.clone());

    let ex4 = pi2_36_2.clone() * pi2_09_2.clone() * t.clone();

    let ex5 =      2 * (pi2.clone() * pi2.clone() * pi2.clone() * pi2.clone());
    let ex6 =    180 * (pi2.clone() * pi2.clone() * pi2.clone()              );
    let ex7 =   5346 * (pi2.clone() * pi2.clone()                            );
    let ex8 =  58320 * (pi2.clone()                                          );
    let ex9 = 209952;

    let ex = ex4 + ex9 + ex8 + ex5 + ex3 + ex2 + ex1 + ex0 + ex7 + ex6;
    -1 * (3 * exp((-t + 2) / 2) * ex) / (8 * pi2_36_2 * pi2_09_2)
}
