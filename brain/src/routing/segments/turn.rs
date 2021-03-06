use crate::{
    behavior::movement::GetToFlatGround,
    eeg::{color, Drawable},
    routing::models::{CarState, CarState2D, SegmentPlan, SegmentRunAction, SegmentRunner},
    strategy::Context,
};
use common::prelude::*;
use nalgebra::{Point2, UnitComplex, Vector2};
use nameof::name_of_type;
use std::f32::consts::PI;

#[derive(Clone)]
pub struct Turn {
    start: CarState2D,
    target_loc: Point2<f32>,
    center: Point2<f32>,
    radius: f32,
    sweep: f32,
}

impl Turn {
    pub fn new(
        start: CarState,
        target_loc: Point2<f32>,
        center: Point2<f32>,
        radius: f32,
        projected_end_loc: Point2<f32>,
    ) -> Self {
        let start = CarState2D {
            loc: start.loc.to_2d(),
            rot: start.rot.to_2d(),
            vel: start.vel.to_2d(),
            boost: 0.0,
        };

        let sweep = (start.loc - center).angle_to(&(projected_end_loc - center));

        Self {
            start,
            target_loc,
            center,
            radius,
            sweep,
        }
    }

    /// Calculate the angle between the two points, traveling in this plan's
    /// direction.
    fn sweep_to(&self, end_loc: Point2<f32>) -> f32 {
        let result = (self.start.loc - self.center).angle_to(&(end_loc - self.center));
        if result < 0.0 && self.sweep >= 0.0 {
            result + 2.0 * PI
        } else if result > 0.0 && self.sweep < 0.0 {
            result - 2.0 * PI
        } else {
            result
        }
    }
}

impl SegmentPlan for Turn {
    fn name(&self) -> &str {
        name_of_type!(Turn)
    }

    fn start(&self) -> CarState {
        self.start.to_3d()
    }

    fn end(&self) -> CarState {
        let sweep = UnitComplex::new(self.sweep);
        CarState2D {
            loc: self.center + sweep * (self.start.loc - self.center),
            rot: sweep * self.start.rot,
            vel: sweep * self.start.vel,
            boost: self.start.boost,
        }
        .to_3d()
    }

    fn duration(&self) -> f32 {
        let assume_speed = f32::max(self.start.vel.norm(), 800.0) * 2.0;
        self.radius * self.sweep.abs() / assume_speed
    }

    fn run(&self) -> Box<dyn SegmentRunner> {
        Box::new(Turner::new(self.clone()))
    }

    fn draw(&self, ctx: &mut Context<'_>) {
        let theta1 = Vector2::x().angle_to(&(self.start.loc - self.center));
        let theta2 = theta1 + self.sweep;
        ctx.eeg.draw(Drawable::Arc(
            self.center,
            self.radius,
            theta1.min(theta2),
            theta1.max(theta2),
            color::YELLOW,
        ));
    }
}

struct Turner {
    plan: Turn,
}

impl Turner {
    pub fn new(plan: Turn) -> Self {
        Self { plan }
    }
}

impl SegmentRunner for Turner {
    fn name(&self) -> &str {
        name_of_type!(Turner)
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> SegmentRunAction {
        let me = ctx.me();
        let me_loc = me.Physics.loc_2d();
        let me_forward = me.Physics.forward_axis_2d();

        if !GetToFlatGround::on_flat_ground(me) {
            ctx.eeg.log(self.name(), "not on flat ground");
            return SegmentRunAction::Failure;
        }

        // Check two end conditions to decrease the chances that silly things happen.

        let yaw_diff = me_forward.angle_to(&(self.plan.target_loc - me_loc).to_axis());
        if yaw_diff.abs() < 3.0_f32.to_radians() {
            ctx.eeg.log(self.name(), "yaw_diff is low enough");
            return SegmentRunAction::Success;
        }

        let swept = self.plan.sweep_to(me_loc);
        let degenerate_sweep = swept.abs() >= PI * (11.0 / 6.0);
        if degenerate_sweep {
            // Our angular momentum seems to be propelling us the wrong way, but don't
            // worry, it will correct itself soon.
            ctx.eeg
                .draw(Drawable::print("degenerate sweep", color::GREEN));
        }
        if !degenerate_sweep && swept.abs() >= self.plan.sweep.abs() - 3.0_f32.to_radians() {
            ctx.eeg.log(self.name(), "sweep has reached target");
            return SegmentRunAction::Success;
        }

        SegmentRunAction::Yield(common::halfway_house::PlayerInput {
            Throttle: 1.0,
            Steer: yaw_diff.signum(),
            ..Default::default()
        })
    }
}
