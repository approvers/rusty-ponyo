use crate::bot::genkai_point::{formula::v2::FormulaV2, model::Session};

pub mod v1;
pub mod v2;

pub(crate) trait GenkaiPointFormula: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn calc(&self, session: &Session) -> u64;
}

pub(crate) fn default_formula() -> impl GenkaiPointFormula {
    FormulaV2
}

pub(crate) struct DynGenkaiPointFormula(pub Box<dyn GenkaiPointFormula>);

impl GenkaiPointFormula for DynGenkaiPointFormula {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn calc(&self, session: &Session) -> u64 {
        self.0.calc(session)
    }
}
