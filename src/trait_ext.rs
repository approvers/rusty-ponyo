pub trait TakeAndApplyIf: Sized {
    fn take_and_apply_if(self, apply: bool, applier: impl FnOnce(Self) -> Self) -> Self;
}

impl<T> TakeAndApplyIf for T {
    fn take_and_apply_if(self, apply: bool, applier: impl FnOnce(Self) -> Self) -> Self {
        if apply {
            applier(self)
        } else {
            self
        }
    }
}
