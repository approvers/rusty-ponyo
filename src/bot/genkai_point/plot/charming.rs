use {
    crate::bot::genkai_point::plot::Plotter,
    anyhow::{anyhow, Result},
    charming::{
        component::Axis, element::name_location::NameLocation, series::Line, Chart, ImageFormat,
        ImageRenderer,
    },
    std::{
        sync::{Arc, Condvar, Mutex},
        thread::{spawn, JoinHandle},
        time::Instant,
    },
};

pub(crate) struct Charming {
    renderer_handle: RendererHandle,
}

impl Charming {
    pub(crate) fn new() -> Self {
        let renderer_handle = spawn_renderer();

        Self { renderer_handle }
    }
}

impl Plotter for Charming {
    fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>> {
        let chart = data
            .iter()
            .fold(Chart::new(), |chart, (label, data)| {
                chart.series(Line::new().name(label).data(data.clone()))
            })
            .background_color("#FFFFFF")
            .x_axis(
                Axis::new()
                    .name_location(NameLocation::Center)
                    .name("時間経過(日)"),
            )
            .y_axis(
                Axis::new()
                    .name_location(NameLocation::Center)
                    .name("累計VC時間(時)"),
            );

        self.renderer_handle.call(chart)
    }
}

struct RendererHandle {
    port: Arc<Mutex<Job>>,
    cond: Arc<Condvar>,

    _handle: JoinHandle<()>,
}

impl RendererHandle {
    fn call(&self, chart: Chart) -> Result<Vec<u8>> {
        let mut guard = self
            .port
            .lock()
            .map_err(|_| anyhow!("failed to lock job queue"))?;

        let chart = Unconsidered::wrap(chart);
        let session = Session::new();

        let Job::Idle = std::mem::replace(&mut *guard, Job::Queued(chart, session)) else {
            unreachable!()
        };

        self.cond.notify_all();

        let mut guard = self
            .cond
            .wait_while(
                guard,
                |job| !matches!(job, Job::Finished(_, s) if session == *s),
            )
            .map_err(|_| anyhow!("failed to lock job queue"))?;

        let Job::Finished(result, _) = std::mem::replace(&mut *guard, Job::Idle) else {
            dbg!(&*guard);
            unreachable!()
        };

        result.unwrap()
    }
}

fn spawn_renderer() -> RendererHandle {
    let port = Arc::new(Mutex::new(Job::Idle));
    let cond = Arc::new(Condvar::new());

    let _handle = {
        let port = port.clone();
        let cond = cond.clone();

        spawn(|| renderer_main(port, cond))
    };

    RendererHandle {
        port,
        cond,
        _handle,
    }
}

fn renderer_main(port: Arc<Mutex<Job>>, cond: Arc<Condvar>) {
    let mut renderer = ImageRenderer::new(1280, 720);

    while let Ok(Ok(mut job)) = port
        .lock()
        .map(|guard| cond.wait_while(guard, |job| !matches!(job, Job::Queued(..))))
    {
        let Job::Queued(arg0, session) = std::mem::replace(&mut *job, Job::Running) else {
            unreachable!()
        };

        let arg0 = arg0.unwrap();
        let ret = renderer
            .render_format(ImageFormat::Png, &arg0)
            .map_err(|_| anyhow!("no detail provided"));

        let ret = Unconsidered::wrap(ret);
        let Job::Running = std::mem::replace(&mut *job, Job::Finished(ret, session)) else {
            unreachable!()
        };

        cond.notify_all();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Job {
    Idle,
    Queued(Unconsidered<Chart>, Session),
    Running,
    Finished(Unconsidered<Result<Vec<u8>>>, Session),
}

struct Unconsidered<T>(T);

impl<T> Unconsidered<T> {
    fn wrap(val: T) -> Self {
        Self(val)
    }

    fn unwrap(self) -> T {
        self.0
    }
}

impl<T> std::fmt::Debug for Unconsidered<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{unconsidered}}")
    }
}

impl<T> PartialEq for Unconsidered<T> {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl<T> Eq for Unconsidered<T> {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Session(Instant);

impl Session {
    fn new() -> Self {
        Self(Instant::now())
    }
}

#[test]
fn test() {
    let result = Charming::new()
        .plot(vec![
            ("kawaemon".into(), vec![1.0, 4.0, 6.0, 7.0]),
            ("kawak".into(), vec![2.0, 5.0, 11.0, 14.0]),
        ])
        .unwrap();

    // should we assert_eq with actual png?
    assert_ne!(result.len(), 0);
}
