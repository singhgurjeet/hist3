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
    pub vals: druid::im::Vector<(f64, f64)>,
    pub x_max: f64,
    pub x_min: f64,
    pub y_max: f64,
    pub y_min: f64,
}

impl Data for AppState {
    fn same(&self, other: &Self) -> bool {
        self.vals.same(&other.vals) && self.x_max.eq(&other.x_max) && self.x_min.eq(&other.x_min) && self.y_max.eq(&other.y_max) && self.y_min.eq(&other.y_min)
    }
}

pub struct Scatter {}

impl Widget<AppState> for Scatter {
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
        let x_data_range = data.x_max - data.x_min;
        let width_by_data_range = width / x_data_range;
        let y_data_range = data.y_max - data.y_min;
        let height_by_data_range = height / y_data_range;
        let num_ticks = 10;

        let rect = Rect::from_origin_size(Point::ORIGIN, size);
        ctx.fill(rect, &DARK_GREY);

        for i in 0..=num_ticks {
            let yy = 5.0 + (i as f64) * (height - 20.0) / (num_ticks as f64);
            let txt = ctx
                .text()
                .new_text_layout(format!(
                    "{:.2}",
                    data.y_max - (i as f64) * y_data_range / (num_ticks as f64)
                ))
                .font(FontFamily::MONOSPACE, 18.0)
                .text_color(BAR_COLOR.clone())
                .build()
                .unwrap();

            ctx.draw_text(&txt, (width - 30.0, yy));
            ctx.stroke(
                Line::new(Point::new(0.0, yy), Point::new(width + 10.0, yy)),
                &LIGHT_GREY,
                0.25,
            );
        }

        for i in 0..=num_ticks {
            let xx = 5.0 + (i as f64) * (width - 20.0) / (num_ticks as f64);
            let txt = ctx
                .text()
                .new_text_layout(format!(
                    "{:.2}",
                    data.x_min + (i as f64) * x_data_range / (num_ticks as f64)
                ))
                .font(FontFamily::MONOSPACE, 18.0)
                .text_color(BAR_COLOR.clone())
                .build()
                .unwrap();

            if i < num_ticks {
                ctx.draw_text(&txt, (xx, height - 10.0));
            }
            ctx.stroke(
                Line::new(Point::new(xx, 0.0), Point::new(xx, height)),
                &LIGHT_GREY,
                0.25,
            );
        }

        for (x,y) in &data.vals {
            let p1 = Point::new(
                5.0 + (x - data.x_min) * width_by_data_range,
                5.0 + height - (y - data.y_min) * height_by_data_range,
            );
            ctx.fill(Circle::new(p1, 2.0), &LINE_COLOR);
        }
    }
}
