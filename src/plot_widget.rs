use druid::Data;

#[derive(Clone, Default, Debug)]
pub struct AppState {
    pub vals: Vec<f64>,
    pub total: f64,
    pub max: f64,
    pub min: f64,
}

impl Data for AppState {
    fn same(&self, other: &Self) -> bool {
        self.vals
            .iter()
            .zip(other.vals.iter())
            .all(|(v, ov)| v.eq(ov))
            && self.total.eq(&other.total)
            && self.max.eq(&other.max)
            && self.min.eq(&other.min)
    }
}
