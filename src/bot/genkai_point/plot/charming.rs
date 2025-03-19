use {
    crate::bot::genkai_point::plot::Plotter,
    anyhow::{Result, anyhow},
    charming::{
        Chart, ImageFormat, ImageRenderer,
        component::{Axis, Legend},
        datatype::{CompositeValue, DataPoint},
        element::{AxisType, name_location::NameLocation},
        series::Line,
        theme::Theme,
    },
    crossbeam::channel::{Receiver, Sender},
    std::thread,
    tokio::sync::oneshot,
};

pub struct Charming {
    renderer: Renderer,
}

impl Charming {
    pub fn new() -> Self {
        let renderer = Renderer::spawn();

        Self { renderer }
    }
}

impl Plotter for Charming {
    async fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>> {
        let chart = data
            .into_iter()
            .fold(Chart::new(), |chart, (label, data)| {
                let data = data
                    .into_iter()
                    .enumerate()
                    .map(|(x, y)| {
                        DataPoint::Value(CompositeValue::Array(vec![(x as i64).into(), y.into()]))
                    })
                    .collect();
                chart.series(Line::new().name(label).data(data))
            })
            .legend(Legend::new().top("bottom"))
            .x_axis(
                Axis::new()
                    .min_interval(1)
                    .type_(AxisType::Value)
                    .name_location(NameLocation::Center)
                    .name("時間経過(日)"),
            )
            .y_axis(
                Axis::new()
                    .type_(AxisType::Value)
                    .name_location(NameLocation::Center)
                    .name("累計VC時間(時)"),
            );

        self.renderer.render(chart).await
    }
}

struct Request {
    data: Chart,
    bell: oneshot::Sender<Response>,
}
struct Response {
    image: Result<Vec<u8>>,
}

struct Renderer {
    tx: Sender<Request>,
    _thread_handle: thread::JoinHandle<()>,
}

impl Renderer {
    fn render_thread(rx: Receiver<Request>) {
        let mut renderer = ImageRenderer::new(1280, 720).theme(Theme::Dark);

        for req in rx {
            let image = renderer
                .render_format(ImageFormat::WebP, &req.data)
                .map_err(|e| anyhow!("charming error: {e:#?}"));

            req.bell.send(Response { image }).ok();
        }
    }

    fn spawn() -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<Request>();

        let handle = std::thread::spawn(|| Self::render_thread(rx));

        Self {
            tx,
            _thread_handle: handle,
        }
    }

    async fn render(&self, data: Chart) -> Result<Vec<u8>> {
        let (tx, rx) = oneshot::channel();

        self.tx.send(Request { data, bell: tx }).unwrap();

        rx.await.unwrap().image
    }
}

#[tokio::test]
async fn test() {
    let charming = std::sync::Arc::new(Charming::new());

    let mut handles = vec![];

    #[allow(unused_variables)]
    for i in 0..10 {
        let charming = charming.clone();

        handles.push(tokio::spawn(async move {
            let result = charming
                .plot(vec![
                    ("kawaemon".into(), vec![1.0, 4.0, 6.0, 7.0]),
                    ("kawak".into(), vec![2.0, 5.0, 11.0, 14.0]),
                ])
                .await
                .unwrap();

            // should we assert_eq with actual png?
            assert_ne!(result.len(), 0);

            // uncomment this to see image artifacts
            // tokio::fs::write(format!("./out{i}.png"), result)
            //     .await
            //     .unwrap();
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}
