use {
    crate::bot::genkai_point::plot::Plotter,
    anyhow::{Context as _, Result},
    ordered_float::OrderedFloat,
    plotters::prelude::*,
};

pub(in crate::bot::genkai_point) struct Plotters;

impl Plotter for Plotters {
    fn plot(&self, data: Vec<(String, Vec<f64>)>) -> Result<Vec<u8>> {
        const SIZE: (usize, usize) = (1280, 720);

        let mut buffer = vec![0; SIZE.0 * SIZE.1 * 3];

        let root = BitMapBackend::with_buffer(&mut buffer, (SIZE.0 as u32, SIZE.1 as u32))
            .into_drawing_area();
        root.fill(&WHITE).context("failed to fill buffer")?;

        let x_range = 0f64..data.first().context("no data in `data`")?.1.len() as f64;
        let y_range = 0f64..(*data
            .iter()
            .flat_map(|x| x.1.last())
            .max_by_key(|&&x| OrderedFloat(x))
            .context("no data in `data")?);

        let mut chart = ChartBuilder::on(&root)
            .margin(20u32)
            .x_label_area_size(40u32)
            .y_label_area_size(60u32)
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
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
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

#[test]
fn test() {
    let result = Plotters.plot(vec![
        ("kawaemon".into(), vec![1.0, 4.0, 6.0, 7.0]),
        ("kawak".into(), vec![2.0, 5.0, 11.0, 14.0]),
    ]);

    // should we assert_eq with actual png?
    assert_ne!(result.unwrap().len(), 0);
}
