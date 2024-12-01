use {crate::bot::genkai_point::model::Session, v3::FormulaV3};

pub mod v1;
pub mod v2;
pub mod v3;

pub trait GenkaiPointFormula: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn calc(&self, sessions: &[Session]) -> GenkaiPointFormulaOutput;
}

pub struct GenkaiPointFormulaOutput {
    pub point: u64,
    pub efficiency: f64,
}

pub fn default_formula() -> impl GenkaiPointFormula {
    FormulaV3
}

pub struct DynGenkaiPointFormula(pub Box<dyn GenkaiPointFormula>);

impl GenkaiPointFormula for DynGenkaiPointFormula {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn calc(&self, sessions: &[Session]) -> GenkaiPointFormulaOutput {
        self.0.calc(sessions)
    }
}
