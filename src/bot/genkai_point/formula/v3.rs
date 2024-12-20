use {
    super::{GenkaiPointFormula, GenkaiPointFormulaOutput},
    crate::bot::genkai_point::model::Session,
    chrono_tz::Asia::Tokyo,
};

pub struct FormulaV3;

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

                let c = (jdt.timestamp_millis() % (24 * ONE_HOUR_MILLIS)) as f64
                    / ONE_HOUR_MILLIS as f64;

                let t = (ldt - jdt).num_milliseconds() as f64 / ONE_HOUR_MILLIS as f64;

                #[rustfmt::skip]
                let now_point = formula(   c, t) - formula(   c, 0.0);
                let max_point = formula(23.0, t) - formula(23.0, 0.0);

                (now_point, max_point)
            })
            .unzip::<_, _, Vec<_>, Vec<_>>();

        let now_point = now_points.iter().sum::<f64>() * 10.0;
        let max_point = max_points.iter().sum::<f64>() * 10.0;

        let point = now_point.round() as u64;
        let efficiency = now_point / max_point;

        GenkaiPointFormulaOutput { point, efficiency }
    }
}

#[rustfmt::skip]
// rendered: https://www.geogebra.org/graphing/esdsm7rz
// used for integrate: https://www.integral-calculator.com
fn formula(c: f64, t: f64) -> f64 {
    let pi = core::f64::consts::PI;

    let sin = f64::sin;
    let cos = f64::cos;
    let exp = f64::exp;
    let pow = f64::powi;

    let pi2 = pow(pi, 2);

    let pi2_36 = pi2 + 36.0;
    let pi2_09 = pi2 +  9.0;

    let pi2_36_2 = pow(pi2_36, 2);
    let pi2_09_2 = pow(pi2_09, 2);

    let tc5_pi_06 = ((t + c + 5.0) * pi /  6.0).rem_euclid(pi * 2.0);
    let tc5_pi_12 = ((t + c + 5.0) * pi / 12.0).rem_euclid(pi * 2.0);

    let ex0 =        pi * pi2_36_2 * (pi2_09 * t + 36.0             ) * sin(tc5_pi_06);
    let ex1 = -3.0 *      pi2_36_2 * (pi2_09 * t + 18.0  - 2.0 * pi2) * cos(tc5_pi_06);
    let ex2 = 48.0 *      pi2_09_2 * (pi2_36 * t + 72.0  - 2.0 * pi2) * sin(tc5_pi_12);
    let ex3 =  8.0 * pi * pi2_09_2 * (pi2_36 * t + 144.0            ) * cos(tc5_pi_12);

    let ex4 = pi2_36_2 * pi2_09_2 * t;

    let ex5 =      2.0 * pow(pi, 8);
    let ex6 =    180.0 * pow(pi, 6);
    let ex7 =   5346.0 * pow(pi, 4);
    let ex8 =  58320.0 * pow(pi, 2);
    let ex9 = 209952.0             ;

    let ex = ex0 + ex1 + ex2 + ex3 + ex4 + ex5 + ex6 + ex7 + ex8 + ex9;
    -1.0 * (3.0 * exp((-t + 2.0) / 2.0) * ex) / (8.0 * pi2_36_2 * pi2_09_2)
}
