use crate::styles::{BAR_COLOR, DARK_GREY, LIGHT_GREY, LINE_COLOR};
use druid::kurbo::Circle;
use druid::kurbo::Line;
use druid::piet::{Text, TextLayoutBuilder};
use druid::{
    BoxConstraints, Data, Env, Event, EventCtx, FontFamily, LayoutCtx, LifeCycle, LifeCycleCtx,
    PaintCtx, Point, Rect, RenderContext, Size, UpdateCtx, Widget,
};

#[derive(Clone, Default, Debug)]
pub struct AppState {
    pub vals: druid::im::Vector<f64>,
    pub max: f64,
    pub min: f64,
}

impl Data for AppState {
    fn same(&self, other: &Self) -> bool {
        self.vals.same(&other.vals) && self.max.eq(&other.max) && self.min.eq(&other.min)
    }
}

pub struct Plot {}

impl Widget<AppState> for Plot {
    fn event(&mut self, _ctx: &mut EventCtx, _event: &Event, _data: &mut AppState, _env: &Env) {}

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
        if data.vals.len() == 0 {
            return;
        }
        let size = ctx.size();
        let width = size.width - 10.0;
        let height = size.height - 10.0;
        let x_delta = width / (data.vals.len() as f64 - 1.0);
        let data_range = data.max - data.min;
        let height_by_data_range = height / data_range;
        let num_ticks = 10;

        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        ctx.fill(rect, &DARK_GREY);

        for i in 0..=num_ticks {
            let yy = 5.0 + (i as f64) * (height - 20.0) / (num_ticks as f64);
            let txt = ctx
                .text()
                .new_text_layout(format!(
                    "{:.2}",
                    data.max - (i as f64) * data_range / (num_ticks as f64)
                ))
                .font(FontFamily::MONOSPACE, 18.0)
                .text_color(BAR_COLOR.clone())
                .build()
                .unwrap();

            ctx.draw_text(&txt, (width - 50.0, yy));
            ctx.stroke(
                Line::new(Point::new(0.0, yy), Point::new(width + 10.0, yy)),
                &LIGHT_GREY,
                0.25,
            );
        }

        for i in 0..data.vals.len() {
            let p1 = Point::new(
                5.0 + x_delta * (i as f64),
                5.0 + height - (data.vals[i] - data.min) * height_by_data_range,
            );
            ctx.fill(Circle::new(p1, 2.0), &LINE_COLOR);
            if i < data.vals.len() - 1 {
                let p2 = Point::new(
                    5.0 + x_delta * ((i + 1) as f64),
                    5.0 + height - (data.vals[i + 1] - data.min) * height_by_data_range,
                );
                ctx.stroke(Line::new(p1, p2), &LINE_COLOR, 2.0);
            }
        }
    }
}
