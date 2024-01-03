use {self::v3::FormulaV3, crate::bot::genkai_point::model::Session};

pub mod v1;
pub mod v2;
pub mod v3;

pub(crate) trait GenkaiPointFormula: Send + Sync + 'static {
    fn name(&self) -> &'static str;
    fn calc(&self, session: &Session) -> u64;
}

pub(crate) fn default_formula() -> impl GenkaiPointFormula {
    FormulaV3
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
