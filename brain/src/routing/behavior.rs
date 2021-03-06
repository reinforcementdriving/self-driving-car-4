use crate::{
    eeg::{color, Drawable},
    routing::models::{
        PlanningContext, ProvisionalPlanExpansion, ProvisionalPlanExpansionTail, RoutePlan,
        RoutePlanError, RoutePlanner, SegmentRunAction, SegmentRunner,
    },
    rules::SameBallTrajectory,
    strategy::{Action, Behavior, Context},
};
use nameof::name_of_type;

pub struct FollowRoute {
    /// Option dance: This only holds a planner before the first tick.
    planner: Option<Box<dyn RoutePlanner>>,
    current: Option<Current>,
    never_recover: bool,
    same_ball_trajectory: Option<SameBallTrajectory>,
}

struct Current {
    plan: RoutePlan,
    runner: Box<dyn SegmentRunner>,
    provisional_expansion_tail: ProvisionalPlanExpansionTail,
}

impl FollowRoute {
    pub fn new(planner: impl RoutePlanner + 'static) -> Self {
        Self::new_boxed(Box::new(planner))
    }

    pub fn new_boxed(planner: Box<dyn RoutePlanner>) -> Self {
        Self {
            planner: Some(planner),
            current: None,
            never_recover: false,
            same_ball_trajectory: None,
        }
    }

    /// A hack to prevent stack overflows. Routes created to recover from
    /// `RoutePlanError` should use this method to prevent recursive recovery.
    pub fn never_recover(mut self, never_recover: bool) -> Self {
        self.never_recover = never_recover;
        self
    }

    pub fn same_ball_trajectory(mut self, same_ball_trajectory: bool) -> Self {
        self.same_ball_trajectory = if same_ball_trajectory {
            Some(SameBallTrajectory::new())
        } else {
            None
        };
        self
    }
}

impl Behavior for FollowRoute {
    fn name(&self) -> &str {
        name_of_type!(FollowRoute)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        if let Some(ref mut same_ball_trajectory) = self.same_ball_trajectory {
            return_some!(same_ball_trajectory.execute_old(ctx));
        }

        if self.current.is_none() {
            let planner = &*self.planner.take().unwrap();
            if let Err(action) = self.advance(planner, ctx) {
                return action;
            }
        }

        self.draw(ctx);
        self.go(ctx)
    }
}

impl FollowRoute {
    fn draw(&mut self, ctx: &mut Context<'_>) {
        // This provisional expansion serves two purposes:
        // 1. Make sure each segment thinks it can complete successfully.
        // 2. Predict far enough ahead that we can draw the whole plan to the screen.
        let current = self.current.as_ref().unwrap();
        let tail = &current.provisional_expansion_tail;
        let provisional_expansion = ProvisionalPlanExpansion::new(&*current.plan.segment, tail);
        for segment in provisional_expansion.iter() {
            segment.draw(ctx);
        }
    }

    fn advance(&mut self, planner: &dyn RoutePlanner, ctx: &mut Context<'_>) -> Result<(), Action> {
        assert!(self.current.is_none());

        ctx.eeg
            .log(self.name(), format!("planning with {}", planner.name()));
        let (plan, log) = match PlanningContext::plan(planner, ctx) {
            Ok((plan, log)) => (plan, log),
            Err(err) => return Err(self.handle_error(ctx, planner.name(), err.error, err.log)),
        };
        ctx.eeg.log(
            self.name(),
            format!("next segment is {}", plan.segment.name()),
        );
        let tail = plan.provisional_expand(&ctx.scenario).map_err(|error| {
            self.handle_error(
                ctx,
                error.planner_name,
                error.error,
                log.into_iter().chain(error.log),
            )
        })?;

        let runner = plan.segment.run();
        self.current = Some(Current {
            plan,
            runner,
            provisional_expansion_tail: tail,
        });
        Ok(())
    }

    fn handle_error(
        &mut self,
        ctx: &mut Context<'_>,
        planner_name: &str,
        error: RoutePlanError,
        log: impl IntoIterator<Item = String>,
    ) -> Action {
        // You can print the plan trace if necessary for debugging
        for line in log.into_iter().take(0) {
            ctx.eeg.log(self.name(), line);
        }

        ctx.eeg.log(
            self.name(),
            format!("error {:?} from planner {}", error, planner_name),
        );

        match error.recover(ctx) {
            Some(b) => {
                if self.never_recover {
                    ctx.eeg
                        .log(self.name(), "recoverable, but we are forbidden to do so");
                    Action::Abort
                } else {
                    ctx.eeg.log(self.name(), "recoverable!");
                    Action::RootCall(b)
                }
            }
            None => {
                ctx.eeg.log(self.name(), "non-recoverable");
                Action::Abort
            }
        }
    }

    fn go(&mut self, ctx: &mut Context<'_>) -> Action {
        let current = self.current.as_mut().unwrap();
        ctx.eeg
            .draw(Drawable::print(current.plan.segment.name(), color::YELLOW));

        let success = match current.runner.execute_old(ctx) {
            SegmentRunAction::Yield(i) => return Action::Yield(i),
            SegmentRunAction::Success => true,
            SegmentRunAction::Failure => false,
        };

        if !success {
            ctx.eeg.log(self.name(), "segment failure; aborting");
            return Action::Abort;
        }

        let current = self.current.take().unwrap();
        let next = some_or_else!(current.plan.next, {
            return Action::Return;
        });
        if let Err(action) = self.advance(&*next, ctx) {
            return action;
        }
        self.go(ctx)
    }
}
