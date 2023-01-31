use {
    crate::bot::genkai_point::plot::Plotter,
    anyhow::{anyhow, Result},
    inline_python::{python, Context as PythonContext},
};

// Plotter implementation using matplotlib via python.
// Note that it seems to matplotlib takes process signal handling
// so Ctrl-C doesn't shutdown this process after calling plot function.

// FIXME: Japanese fonts rendering are broken.
// FIXME: No axis description.

pub(crate) struct Matplotlib {}

impl Matplotlib {
    pub(crate) fn new() -> Self {
        Self {}
    }
}

impl Plotter for Matplotlib {
    fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>> {
        let result: Result<PythonContext, _> = std::panic::catch_unwind(|| {
            python! {
                import io
                from matplotlib import pyplot

                figure = pyplot.figure()

                for k, v in 'data:
                    pyplot.plot(list(range(1, len(v) + 1)), v, label=k)

                figure.legend(loc="lower right")

                buffer = io.BytesIO()
                figure.savefig(buffer)

                result = buffer.getvalue()
            }
        });

        match result {
            Ok(v) => Ok(v.get("result")),
            Err(_) => Err(anyhow!("failed to plot graph")),
        }
    }
}

#[test]
fn test_plot_to_image() {
    let result = Matplotlib.plot(vec![
        ("kawaemon".into(), vec![1.0, 4.0, 6.0, 7.0]),
        ("kawak".into(), vec![2.0, 5.0, 11.0, 14.0]),
    ]);

    // should we assert_eq with actual png?
    assert_ne!(result.unwrap().len(), 0);
}
