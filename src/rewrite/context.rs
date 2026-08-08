use crate::plan::{InternalVarAllocator, PlanVar};

#[derive(Debug, Default)]
pub(crate) struct RewriteContext {
    variables: InternalVarAllocator,
}

impl RewriteContext {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn fresh_var(&mut self) -> PlanVar {
        PlanVar::Internal(self.variables.fresh())
    }
}
