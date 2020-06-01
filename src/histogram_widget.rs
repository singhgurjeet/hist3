use crate::AppState;
use crate::styles::*;

use druid::piet::{FontBuilder, ImageFormat, InterpolationMode, Text, TextLayoutBuilder};
use druid::widget::prelude::*;
use druid::{Affine, AppLauncher, Color, LocalizedString, Point, Rect, WindowDesc};

pub struct Histogram {}

impl Widget<AppState> for Histogram {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut AppState, _env: &Env) {}

    fn lifecycle(&mut self, _ctx: &mut LifeCycleCtx, _event: &LifeCycle, _data: &AppState, _env: &Env) {}

    fn update(&mut self, _ctx: &mut UpdateCtx, _old_data: &AppState, _data: &AppState, _env: &Env) {}

    fn layout(&mut self, _layout_ctx: &mut LayoutCtx, bc: &BoxConstraints, _data: &AppState, _env: &Env) -> Size {
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
        let max_count = data.labels_and_counts.iter()
            .map(|(_, i)| i)
            .max_by(|x, y| x.cmp(y)).unwrap_or(&(0 as usize));
        let height_per_count = height / (*max_count as f64);

        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        ctx.fill(rect, &DARK_GREY);
        data.labels_and_counts.iter().enumerate().for_each(|(i, (_, c))| {
            let r = Rect::from_origin_size(
                Point::new((i as f64) * bar_width, height - (*c as f64) * height_per_count),
                Size::new(bar_width, (*c as f64) * height_per_count));
            ctx.fill(r, &BAR_COLOR);
            ctx.stroke(r, &DARK_GREY, 0.25);
        });
    }
}
