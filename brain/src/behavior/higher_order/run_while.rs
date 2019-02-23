use crate::{
    eeg::{color, Drawable},
    strategy::{Action, Behavior, Context, Priority},
};

/// Run `child` while `predicate` holds true.
pub struct While<P, B>
where
    P: Predicate,
    B: Behavior,
{
    predicate: P,
    child: B,
}

pub trait Predicate: Send {
    fn name(&self) -> &str;
    fn evaluate(&mut self, ctx: &mut Context<'_>) -> bool;
}

impl<P, B> While<P, B>
where
    P: Predicate,
    B: Behavior,
{
    pub fn new(predicate: P, child: B) -> Self {
        Self { predicate, child }
    }
}

impl<P, B> Behavior for While<P, B>
where
    P: Predicate,
    B: Behavior,
{
    fn name(&self) -> &str {
        stringify!(While)
    }

    fn priority(&self) -> Priority {
        self.child.priority()
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        if !self.predicate.evaluate(ctx) {
            ctx.eeg.log(self.name(), "terminating");
            return Action::Return;
        }

        ctx.eeg
            .draw(Drawable::print(self.child.blurb(), color::YELLOW));

        self.child.execute_old(ctx)
    }
}
