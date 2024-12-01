use {
    crate::bot::genkai_point::plot::Plotter,
    anyhow::{Context as _, Result},
    ordered_float::OrderedFloat,
    plotters::prelude::*,
};

crate::assert_one_feature!("plot_plotters_static", "plot_plotters_dynamic");

pub struct Plotters {}

impl Plotters {
    pub fn new() -> Self {
        #[cfg(feature = "plot_plotters_static")]
        {
            use parking_lot::Once;
            static REGISTER_FONT: Once = Once::new();
            REGISTER_FONT.call_once(|| {
                plotters::style::register_font(
                    "sans-serif",
                    FontStyle::Normal,
                    include_bytes!("../../../../NotoSansCJKjp-Medium.ttf"), // RUN download_font.sh IF YOU GOT THE NOT FOUND ERROR HERE
                )
                // this error doesn't implement Debug
                .map_err(|_| panic!("failed to load embedded font"))
                .unwrap()
            });
        }

        Self {}
    }
}

impl Plotter for Plotters {
    async fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>> {
        const SIZE: (usize, usize) = (1280, 720);

        let mut buffer = vec![0; SIZE.0 * SIZE.1 * 3];

        let root =
            BitMapBackend::with_buffer(&mut buffer, (SIZE.0 as _, SIZE.1 as _)).into_drawing_area();
        root.fill(&WHITE).context("failed to fill buffer")?;

        let x_range = 0.0..data.first().context("no data in `data`")?.1.len() as f64;
        let y_range = 0.0..(*data
            .iter()
            .flat_map(|x| x.1.last())
            .max_by_key(|&&x| OrderedFloat(x))
            .context("no data in `data")?);

        let mut chart = ChartBuilder::on(&root)
            .margin(20)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(x_range, y_range)
            .context("failed to build chart")?;

        chart
            .configure_mesh()
            .x_desc("時間経過(日)")
            .y_desc("累計VC時間(時)")
            .axis_desc_style(("sans-serif", 15))
            .draw()?;

        for (i, (label, data)) in data.into_iter().enumerate() {
            let color = Palette99::COLORS[i % Palette99::COLORS.len()];
            let color = RGBColor(color.0, color.1, color.2);

            chart
                .draw_series(LineSeries::new(
                    data.into_iter().enumerate().map(|(a, b)| (a as f64, b)),
                    ShapeStyle {
                        color: color.to_rgba(),
                        filled: true,
                        stroke_width: 3,
                    },
                ))
                .context("failed to draw series")?
                .label(label)
                .legend(move |(x, y)| Rectangle::new([(x - 5, y - 5), (x + 5, y + 5)], color));
        }

        chart
            .configure_series_labels()
            .background_style(WHITE.mix(0.8))
            .border_style(BLACK)
            .draw()
            .context("failed to draw series labels")?;

        // to borrow buffer later
        drop(chart);
        drop(root);

        let mut output = vec![];

        let mut encoder = png::Encoder::new(&mut output, SIZE.0 as _, SIZE.1 as _);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Best);

        encoder
            .write_header()
            .context("failed to write png headers")?
            .write_image_data(&buffer)
            .context("failed to write png image data")?;

        Ok(output)
    }
}

#[tokio::test]
async fn test() {
    let result = Plotters::new()
        .plot(vec![
            ("kawaemon".into(), vec![1.0, 4.0, 6.0, 7.0]),
            ("kawak".into(), vec![2.0, 5.0, 11.0, 14.0]),
        ])
        .await
        .unwrap();

    // should we assert_eq with actual png?
    assert_ne!(result.len(), 0);
}
