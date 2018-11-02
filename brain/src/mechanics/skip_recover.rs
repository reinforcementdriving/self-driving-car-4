use behavior::{Action, Behavior};
use common::{physics::CAR_LOCAL_FORWARD_AXIS_2D, prelude::*};
use eeg::Drawable;
use nalgebra::{Point2, UnitComplex};
use strategy::Context;

#[derive(new)]
pub struct SkidRecover {
    target_loc: Point2<f32>,
}

impl Behavior for SkidRecover {
    fn name(&self) -> &str {
        stringify!(SkidRecover)
    }

    fn execute2(&mut self, ctx: &mut Context) -> Action {
        let me = ctx.me();
        let me_rot = me.Physics.quat().to_2d();
        let me_ang_vel = me.Physics.ang_vel().z;
        let me_to_target = self.target_loc - me.Physics.loc_2d();

        let target_rot = CAR_LOCAL_FORWARD_AXIS_2D.rotation_to(&me_to_target.to_axis());
        // Since we're skidding, aim towards where we will be a bit in the future.
        // Otherwise we'll overshoot.
        let future_rot = target_rot * UnitComplex::new(me_ang_vel * 0.25);
        let steer = me_rot.rotation_to(&future_rot).angle().max(-1.0).min(1.0);

        ctx.eeg.draw(Drawable::ghost_car_ground(
            self.target_loc.coords,
            target_rot.around_z_axis().to_rotation_matrix(),
        ));

        Action::Yield(rlbot::ffi::PlayerInput {
            Throttle: 1.0,
            Steer: steer,
            ..Default::default()
        })
    }
}
