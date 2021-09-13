use crate::styles::*;

use druid::kurbo::Line;
use druid::piet::{Text, TextLayoutBuilder};
use druid::widget::prelude::*;
use druid::{Point, Rect, Data, FontFamily};

//TODO: Add a vertical line with the current value of the x axis as the pointer moves (white on black)
//TODO: Add a command line option to disable the legend

#[derive(Clone, Default, Debug)]
pub struct AppState {
    pub loaded: bool,
    pub labels_and_counts: Vec<(String, usize)>,
    pub p_25: Option<f64>,
    pub p_50: Option<f64>,
    pub p_75: Option<f64>,
    pub total: f64,
    pub highlight: Option<usize>,
}

impl Data for AppState {
    fn same(&self, other: &Self) -> bool {
        self.loaded.eq(&other.loaded)
            && self.p_25.eq(&other.p_25)
            && self.p_50.eq(&other.p_50)
            && self.p_75.eq(&other.p_75)
            && self.total.eq(&other.total)
            && self.highlight.eq(&other.highlight)
            && self
                .labels_and_counts
                .iter()
                .zip(other.labels_and_counts.iter())
                .all(|((s, i), (os, oi))| s.eq(os) && i.eq(oi))
    }
}

pub struct Histogram {}

impl Widget<AppState> for Histogram {
    fn event(&mut self, ctx: &mut EventCtx, event: &Event, data: &mut AppState, _env: &Env) {
        match event {
            Event::MouseMove(e) => {
                let width = ctx.size().width;
                data.highlight =
                    Some(((data.labels_and_counts.len() as f64) * e.pos.x / width) as usize);
            }
            _ => {}
        }
    }

    fn lifecycle(
        &mut self,
        _ctx: &mut LifeCycleCtx,
        _event: &LifeCycle,
        _data: &AppState,
        _env: &Env,
    ) {
    }

    fn update(&mut self, ctx: &mut UpdateCtx, _old_data: &AppState, _data: &AppState, _env: &Env) {
        ctx.request_paint();
    }

    fn layout(
        &mut self,
        _layout_ctx: &mut LayoutCtx,
        bc: &BoxConstraints,
        _data: &AppState,
        _env: &Env,
    ) -> Size {
        // BoxConstraints are passed by the parent widget.
        // This method can return any Size within those constraints:
        // bc.constrain(my_size)
        //
        // To check if a dimension is infinite or not (e.g. scrolling):
        // bc.is_width_bounded() / bc.is_height_bounded()
        bc.max()
    }

    // The paint method gets called last, after an event flow.
    // It goes event -> update -> layout -> paint, and each method can influence the next.
    // Basically, anything that changes the appearance of a widget causes a paint.
    fn paint(&mut self, ctx: &mut PaintCtx, data: &AppState, _env: &Env) {
        // Clear the whole widget with the color of your choice
        // (ctx.size() returns the size of the layout rect we're painting in)
        let size = ctx.size();
        let width = size.width;
        let height = size.height;
        let num_bins = data.labels_and_counts.len() as f64;
        let bar_width = width / num_bins;
        let max_count = data
            .labels_and_counts
            .iter()
            .map(|(_, i)| i)
            .max_by(|x, y| x.cmp(y))
            .unwrap_or(&(0 as usize));
        let height_per_count = height / (*max_count as f64);

        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        ctx.fill(rect, &DARK_GREY);

        if let Some(p) = data.p_25 {
            ctx.stroke(
                Line::new(Point::new(p * width, 0.0), Point::new(p * width, height)),
                &LIGHT_GREY,
                0.5,
            );
        }
        if let Some(p) = data.p_50 {
            ctx.stroke(
                Line::new(Point::new(p * width, 0.0), Point::new(p * width, height)),
                &LIGHT_GREY,
                0.5,
            );
        }
        if let Some(p) = data.p_75 {
            ctx.stroke(
                Line::new(Point::new(p * width, 0.0), Point::new(p * width, height)),
                &LIGHT_GREY,
                0.5,
            );
        }

        data.labels_and_counts
            .iter()
            .enumerate()
            .for_each(|(i, (_, c))| {
                let r = Rect::from_origin_size(
                    Point::new(
                        (i as f64) * bar_width,
                        height - (*c as f64) * height_per_count,
                    ),
                    Size::new(bar_width, (*c as f64) * height_per_count),
                );
                if data.highlight == Some(i) {
                    let count = ctx
                        .text()
                        .new_text_layout(format!("{:.2}", data.labels_and_counts[i].1.clone()))
                        .font(FontFamily::MONOSPACE, 18.0)
                        .text_color(BAR_COLOR.clone())
                        .build()
                        .unwrap();
                    let pct = ctx
                        .text()
                        .new_text_layout(format!(
                            "{:.2}%",
                            100.0 * (data.labels_and_counts[i].1.clone() as f64) / data.total
                        ))
                        .font(FontFamily::MONOSPACE, 18.0)
                        .text_color(BAR_COLOR.clone())
                        .build()
                        .unwrap();
                    let val = ctx
                        .text()
                        .new_text_layout(data.labels_and_counts[i].0.clone())
                        .font(FontFamily::MONOSPACE, 18.0)
                        .text_color(BAR_COLOR.clone())
                        .build()
                        .unwrap();
                    ctx.draw_text(&count, (0.0, 18.0));
                    ctx.draw_text(&pct, (0.0, 36.0));
                    ctx.draw_text(&val, (0.0, 54.0));

                    ctx.fill(r, &HIGHLIGHT_BAR_COLOR);
                } else {
                    ctx.fill(r, &BAR_COLOR);
                }
                ctx.stroke(r, &DARK_GREY, 0.25);
            });
    }
}
