use crate::{
    behavior::{
        movement::{Dodge, JumpAndTurn, Yielder},
        strike::grounded_hit::car_ball_contact_with_pitch,
    },
    eeg::{Event, EEG},
    helpers::ball::BallFrame,
    routing::models::CarState,
    strategy::{Action, Behavior, Context, Context2, Priority},
    utils::{
        geometry::flattener::Flattener,
        intercept_memory::{InterceptMemory, InterceptMemoryResult},
    },
};
use common::{
    kinematics::kinematic,
    physics::{car_forward_axis_2d, CAR_LOCAL_FORWARD_AXIS_2D},
    prelude::*,
    rl,
};
use nalgebra::{Isometry3, Point2, Point3, UnitComplex, UnitQuaternion, Vector3};
use nameof::name_of_type;
use simulate::{car_jump, Car1D};
use std::f32::consts::PI;

pub struct WallHit {
    intercept: InterceptMemory,
}

impl WallHit {
    pub const MAX_BALL_DISTANCE_FROM_SURFACE: f32 = 300.0;

    pub fn new() -> Self {
        Self {
            intercept: InterceptMemory::new(),
        }
    }
}

impl Behavior for WallHit {
    fn name(&self) -> &str {
        name_of_type!(WallHit)
    }

    fn priority(&self) -> Priority {
        Priority::Strike
    }

    fn execute_old(&mut self, ctx: &mut Context<'_>) -> Action {
        let (ref ctx, ref mut eeg) = ctx.split();

        if !ctx.me().OnGround {
            eeg.log(self.name(), "not on ground");
            return Action::Abort;
        }

        let intercept = some_or_else!(intercept(ctx), {
            eeg.log(self.name(), "no viable intercept");
            return Action::Abort;
        });

        let intercept_time = intercept.t;
        let now = ctx.packet.GameInfo.TimeSeconds;
        let intercept_ball_loc = match self.intercept.update(now, intercept.loc, eeg) {
            InterceptMemoryResult::Stable(loc) => loc,
            InterceptMemoryResult::Unstable(loc) => {
                eeg.log(self.name(), "trying unstable intercept");
                loc
            }
        };

        let path = match flat_target(ctx, eeg, &intercept_ball_loc) {
            Ok(x) => x,
            Err(()) => {
                eeg.log(self.name(), "error finding target");
                return Action::Abort;
            }
        };

        match calculate_approach(ctx, eeg, intercept_time, &path) {
            Step::Drive(throttle, boost) => drive(ctx.me(), &path, throttle, boost),
            Step::Jump => jump(eeg, &path),
        }
    }
}

fn intercept<'ctx>(ctx: &'ctx Context2<'_, '_>) -> Option<&'ctx BallFrame> {
    for ball in ctx.scenario.ball_prediction().iter() {
        if let Ok(()) = check_intercept(&ctx, ball) {
            return Some(ball);
        }
    }
    None
}

fn check_intercept(ctx: &Context2<'_, '_>, ball: &BallFrame) -> Result<(), ()> {
    const RADII: f32 = 200.0; // TODO: tune

    let me = ctx.me();
    let me_surface = ctx.game.pitch().closest_plane(&me.Physics.loc());
    let target = ball.loc;
    let target_surface = ctx.game.pitch().closest_plane(&target);
    let ground = ctx.game.pitch().ground();

    if target_surface.normal.y == ctx.game.enemy_goal().normal_2d.y {
        // HACK: Suppress enemy wall hits, they are almost never useful.
        return Err(());
    }

    let me_to_ground = me_surface.unfold(&ground)?;
    let target_to_me = target_surface.unfold(&me_surface)?;
    let target_to_ground = me_to_ground * target_to_me;

    let me_to_flat = Flattener::new(me_to_ground);
    let target_to_flat = Flattener::new(target_to_ground);

    let flat_start_loc = me_to_flat * me.Physics.loc();
    let flat_start_vel = me_to_flat * me.Physics.vel();
    let flat_target_loc = target_to_flat * target;
    let flat_dist = (flat_target_loc - flat_start_loc).norm();

    let mut sim_car = Car1D::new()
        .with_speed(flat_start_vel.norm())
        .with_boost(me.Boost as f32);
    sim_car.advance(ball.t, 1.0, true);

    if sim_car.distance() < flat_dist - RADII {
        return Err(());
    }

    Ok(())
}

fn flat_target(
    ctx: &Context2<'_, '_>,
    eeg: &mut EEG,
    intercept_ball_loc: &Point3<f32>,
) -> Result<Path, ()> {
    let me = ctx.me();
    let me_surface = ctx.game.pitch().closest_plane(&me.Physics.loc());
    let intercept_surface = ctx.game.pitch().closest_plane(&intercept_ball_loc);
    let ground = ctx.game.pitch().ground();

    if me_surface.normal == ground.normal
        && intercept_surface.normal == ground.normal
        && me.Physics.roof_axis().angle_to(&Vector3::z_axis()).abs() < 15.0_f32.to_radians()
    {
        // Other behaviors are better at ground play, so step out of the way so one of
        // them can take over.
        eeg.log(name_of_type!(WallHit), "no walls are involved");
        return Err(());
    }

    // Build the origami structure
    let me_to_ground = me_surface.unfold(&ground)?;
    let intercept_to_me = intercept_surface.unfold(&me_surface)?;
    let intercept_to_ground = me_to_ground * intercept_to_me;
    let ground_to_intercept = intercept_to_ground.inverse();

    let me_to_flat = Flattener::new(me_to_ground);
    let intercept_to_flat = Flattener::new(intercept_to_ground);

    // "Unfold" the path onto the ground
    let ground_start_loc = me_to_ground * me.Physics.loc();
    let ground_intercept_ball_loc = intercept_to_ground * intercept_ball_loc;

    // Make sure we're not facing wildly the wrong direction
    let me_forward = me_to_flat * me.Physics.forward_axis();
    let me_to_ball = intercept_to_flat * *intercept_ball_loc - me_to_flat * me.Physics.loc();
    let steer = me_forward.angle_to(&me_to_ball.to_axis());
    if steer.abs() >= PI / 3.0 {
        eeg.track(Event::WallHitNotFacingTarget);
        eeg.log(name_of_type!(WallHit), "not facing the target");
        return Err(());
    }

    if ground_intercept_ball_loc.z >= WallHit::MAX_BALL_DISTANCE_FROM_SURFACE {
        eeg.log(name_of_type!(WallHit), "intercept is too far from surface");
        return Err(());
    }

    let (ground_target_loc, ground_target_rot) = car_ball_contact_with_pitch(
        ctx.game,
        ground_intercept_ball_loc,
        ground_start_loc,
        PI / 12.0,
    );

    assert!(ground.offset == 0.0); // intercept_distance_from_surface relies on this

    Ok(Path {
        intercept_distance_from_surface: ground_intercept_ball_loc.z,
        target_loc: ground_to_intercept * ground_target_loc,
        target_rot: ground_to_intercept.rotation * ground_target_rot,

        start_to_flat: me_to_flat,
        target_to_flat: intercept_to_flat,
        flat_to_target: intercept_to_flat.inverse(),

        flat_start_loc: ground_start_loc.to_2d(),
        flat_start_rot: me_to_flat * me.Physics.quat(),
        flat_target_loc: ground_target_loc.to_2d(),
        ground_start_loc,
        ground_target_loc,
    })
}

struct Path {
    // World coordinates
    intercept_distance_from_surface: f32,
    target_loc: Point3<f32>,
    target_rot: UnitQuaternion<f32>,

    // Unroll transform
    start_to_flat: Flattener,
    target_to_flat: Flattener,
    flat_to_target: Isometry3<f32>,

    // Unrolled coordinates
    flat_start_loc: Point2<f32>,
    flat_start_rot: UnitComplex<f32>,
    flat_target_loc: Point2<f32>,
    ground_start_loc: Point3<f32>,
    ground_target_loc: Point3<f32>,
}

#[allow(clippy::if_same_then_else)]
fn calculate_approach(
    ctx: &Context2<'_, '_>,
    eeg: &mut EEG,
    target_time: f32,
    path: &Path,
) -> Step {
    let (jump_distance, jump_time) = calculate_jump(path);
    let drive_time = target_time - jump_time;

    if drive_time < 0.0 {
        return Step::Jump;
    }

    let drive = SimDrive {
        start_to_flat: path.start_to_flat,
        target_to_flat: path.target_to_flat,
        flat_to_target: path.flat_to_target,

        target_loc: path.ground_target_loc,
    };

    let jump = SimJump;

    let trial = |throttle, boost| {
        let state = ctx.me().into();
        let state = drive.simulate(&state, drive_time, throttle, boost);
        let state = jump.simulate(&state, jump_time, &path.target_rot);

        let flat_end_loc = path.target_to_flat * state.loc;
        let flat_dist = (flat_end_loc - path.flat_start_loc).norm();
        let target_dist = (path.flat_target_loc - path.flat_start_loc).norm();
        flat_dist - target_dist
    };

    // Aim for a few uu behind the ball so we don't make contact before we dodge.
    let target_offset = -10.0;

    let coast_offset = trial(0.0, false);
    let throttle_offset = trial(1.0, false);
    let blitz_offset = trial(1.0, true);

    let (throttle, boost) = if coast_offset > target_offset {
        (0.0, false) // We're overshooting…
    } else if throttle_offset > target_offset {
        (0.0, false)
    } else if blitz_offset > target_offset {
        (1.0, false)
    } else {
        (1.0, true)
    };

    eeg.print_value("target", path.target_loc);
    eeg.print_value("flat_loc", path.ground_start_loc);
    eeg.print_value("flat_target", path.ground_target_loc);
    eeg.print_distance("jump_distance", jump_distance);
    eeg.print_distance("intercept_elevation", path.intercept_distance_from_surface);
    eeg.print_time("drive_time", drive_time);
    eeg.print_time("jump_time", jump_time);
    eeg.print_time("total_time", target_time);
    eeg.print_distance("coast_offset", coast_offset);
    eeg.print_distance("throttle_offset", throttle_offset);
    eeg.print_distance("blitz_offset", blitz_offset);

    Step::Drive(throttle, boost)
}

fn calculate_jump(path: &Path) -> (f32, f32) {
    let jump_distance = path.ground_target_loc.z - rl::OCTANE_NEUTRAL_Z;
    let jump_time = car_jump::jump_duration(&path.target_rot, jump_distance.max(0.001)).unwrap();
    assert!(jump_time < 1.0, "{}", jump_time);
    (jump_distance, jump_time)
}

enum Step {
    /// `(throttle, boost)`
    Drive(f32, bool),
    Jump,
}

fn drive(
    me: &common::halfway_house::PlayerInfo,
    path: &Path,
    throttle: f32,
    boost: bool,
) -> Action {
    let flat_forward_axis = car_forward_axis_2d(path.flat_start_rot);
    let steer = flat_forward_axis.angle_to(&(path.flat_target_loc - path.flat_start_loc).to_axis());
    Action::Yield(common::halfway_house::PlayerInput {
        Throttle: throttle,
        Steer: (steer * 2.0).max(-1.0).min(1.0),
        Boost: boost && me.Physics.vel().norm() < rl::CAR_ALMOST_MAX_SPEED,
        ..Default::default()
    })
}

fn jump(eeg: &mut EEG, path: &Path) -> Action {
    let (_jump_distance, jump_time) = calculate_jump(path);

    // If the ball is very close to the wall, don't jump; instead, just chip it off
    // the wall. This way we retain more control of our car.
    if path.intercept_distance_from_surface < rl::BALL_RADIUS + 25.0 {
        eeg.track(Event::WallHitFinishedWithoutJump);
        // At this point we haven't _quite_ made contact yet (since we're
        // skipping the time where we assumed we would be jumping). Follow
        // through for maximum power.
        return Action::tail_call(Yielder::new(
            jump_time + 0.05,
            common::halfway_house::PlayerInput {
                Throttle: 1.0,
                ..Default::default()
            },
        ));
    }

    Action::tail_call(chain!(Priority::Strike, [
        JumpAndTurn::new(
            (jump_time - 0.05).min(rl::CAR_JUMP_FORCE_TIME),
            (jump_time - 0.05).min(rl::CAR_JUMP_FORCE_TIME) + 0.05,
            path.target_rot,
        ),
        Dodge::new(),
    ]))
}

struct SimDrive {
    // Coordinate system
    start_to_flat: Flattener,
    target_to_flat: Flattener,
    flat_to_target: Isometry3<f32>,

    // Maneuver
    target_loc: Point3<f32>,
}

impl SimDrive {
    fn simulate(&self, start: &CarState, time: f32, throttle: f32, boost: bool) -> CarState {
        let flat_start_loc = self.start_to_flat * start.loc;
        let flat_start_vel = self.start_to_flat * start.vel;
        let flat_target_loc = self.target_to_flat * self.target_loc;
        let flat_dir = (flat_target_loc - flat_start_loc).to_axis();

        let mut car = Car1D::new()
            .with_speed(flat_start_vel.norm())
            .with_boost(start.boost);
        car.advance(time, throttle, boost);

        let flat_end_loc = flat_start_loc + flat_dir.into_inner() * car.distance();
        let flat_end_rot = CAR_LOCAL_FORWARD_AXIS_2D.rotation_to(&flat_dir);
        let flat_end_vel = flat_dir.into_inner() * car.speed();

        CarState {
            loc: self.flat_to_target * flat_end_loc.to_3d(rl::OCTANE_NEUTRAL_Z),
            rot: self.flat_to_target.rotation * flat_end_rot.around_z_axis(),
            vel: self.flat_to_target * flat_end_vel.to_3d(0.0),
            boost: car.boost(),
        }
    }
}

struct SimJump;

impl SimJump {
    fn simulate(&self, start: &CarState, time: f32, target_rot: &UnitQuaternion<f32>) -> CarState {
        let force_time = time.min(rl::CAR_JUMP_FORCE_TIME);
        let v_0 = start.vel + start.roof_axis().into_inner() * rl::CAR_JUMP_IMPULSE_SPEED;
        let a = start.roof_axis().into_inner() + Vector3::z() * rl::GRAVITY;
        let (d, vel) = kinematic(v_0, a, force_time);
        let loc = start.loc + d;

        let coast_time = (force_time - rl::CAR_JUMP_FORCE_TIME).max(0.0);
        let a = Vector3::z() * rl::GRAVITY;
        let (d, vel) = kinematic(vel, a, coast_time);
        let loc = loc + d;

        CarState {
            loc,
            rot: *target_rot,
            vel,
            boost: start.boost,
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        behavior::strike::WallHit,
        eeg::Event,
        integration_tests::{TestRunner, TestScenario},
    };
    use common::prelude::*;
    use nalgebra::{Point3, Rotation3, Vector3};
    use std::f32::consts::PI;

    #[test]
    fn side_wall_high() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-3000.0, 0.0, 90.0),
                ball_vel: Vector3::new(-2000.0, 0.0, 0.0),
                car_loc: Point3::new(-3000.0, -2000.0, 17.01),
                car_rot: Rotation3::from_unreal_angles(0.0, PI * 0.75, 0.0),
                ..Default::default()
            })
            .behavior(WallHit::new())
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().y >= 1000.0);
    }

    #[test]
    fn side_wall_low() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(3782.89, 563.18, 93.14),
                ball_vel: Vector3::new(1426.631, -608.53094, 0.0),
                ball_ang_vel: Vector3::new(2.53371, 5.33531, 1.05531),
                car_loc: Point3::new(2422.1099, -635.58997, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009912956, 0.29817224, 0.000076749064),
                car_vel: Vector3::new(578.16095, 173.541, 8.301001),
                ..Default::default()
            })
            .behavior(WallHit::new())
            .run_for_millis(2000);

        let packet = test.sniff_packet();
        println!("ball vel = {:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x < -500.0);
        assert!(packet.GameBall.Physics.vel().y >= 100.0);
    }

    #[test]
    fn side_wall_easy_angle() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(3971.1199, -1644.1699, 1230.4299),
                ball_vel: Vector3::new(-19.700998, 948.86096, 203.451),
                car_loc: Point3::new(3673.13, -2857.3, 16.84),
                car_rot: Rotation3::from_unreal_angles(-0.009273871, 1.6484088, -0.0004669602),
                car_vel: Vector3::new(5.8809996, 379.32098, 11.741),
                ..Default::default()
            })
            .behavior(WallHit::new())
            .run_for_millis(2000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().y >= 1000.0);
    }

    #[test]
    fn from_the_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-3135.25, -3098.04, 106.81),
                ball_vel: Vector3::new(-1808.0609, 1254.9609, -115.760994),
                car_loc: Point3::new(-2169.3699, -4622.36, 16.97),
                car_rot: Rotation3::from_unreal_angles(-0.009957742, -3.1173651, 0.00029480227),
                car_vel: Vector3::new(-1587.4609, -12.1310005, 8.981),
                ..Default::default()
            })
            .behavior(WallHit::new())
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().y >= 1000.0);
    }

    #[test]
    fn angle_check_dont_bail() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-3500.0, 1600.0, 98.0),
                ball_vel: Vector3::new(-1800.0, 1000.0, 0.0),
                car_loc: Point3::new(-3500.0, 1000.0, 17.01),
                car_rot: Rotation3::from_unreal_angles(0.0, 3.1, 0.0),
                car_vel: Vector3::new(-1800.0, 200.0, 0.0),
                ..Default::default()
            })
            .starting_boost(0.0)
            .behavior(WallHit::new())
            .run_for_millis(2000);

        let packet = test.sniff_packet();
        println!("ball vel = {:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().y >= 1000.0);
        test.examine_events(|events| {
            assert!(events.contains(&Event::WallHitFinishedWithoutJump));
            assert!(!events.contains(&Event::WallHitNotFacingTarget));
        });
    }
}
