use crate::{
    behavior::{
        defense::retreat::Retreat,
        higher_order::Chain,
        offense::TepidHit,
        strike::{
            BounceShot, GroundedHit, GroundedHitAimContext, GroundedHitTarget,
            GroundedHitTargetAdjust,
        },
    },
    eeg::{color, Drawable, Event},
    predict::naive_ground_intercept_2,
    routing::{behavior::FollowRoute, plan::GroundIntercept},
    strategy::{Action, Behavior, Context, Goal, Priority, Scenario},
    utils::{geometry::ExtendF32, Wall, WallRayCalculator},
};
use common::prelude::*;
use nalgebra::{Point2, Point3, Rotation2, Vector2};
use nameof::name_of_type;
use ordered_float::NotNan;
use std::f32::consts::PI;

pub struct Defense;

impl Defense {
    pub fn new() -> Defense {
        Defense
    }

    fn is_between_ball_and_own_goal(ctx: &mut Context) -> bool {
        let goal_loc = ctx.game.own_goal().center_2d;
        let me_loc = ctx.me().Physics.loc_2d();
        let ball_loc = match ctx.scenario.me_intercept() {
            Some(i) => i.ball_loc.to_2d(),
            None => ctx.scenario.ball_prediction().last().loc.to_2d(),
        };
        let goal_to_ball_axis = (ball_loc - goal_loc).to_axis();

        let ball_dist = (ball_loc - goal_loc).dot(&goal_to_ball_axis);
        let me_dist = (me_loc - goal_loc).dot(&goal_to_ball_axis);
        if ball_dist <= me_dist {
            return false;
        }

        let defending_angle = (ball_loc - goal_loc).rotation_to(me_loc - goal_loc);
        if defending_angle.angle().abs() >= PI / 3.0 {
            // If we're in net, chances are our angle of defense is fine already. e.g. we
            // might be opposite the desired angle, which would be 180° away according to
            // the math, but is a perfectly fine place to be.
            if (me_loc.y - goal_loc.y).abs() >= 500.0 {
                return false;
            }
        }

        true
    }
}

impl Behavior for Defense {
    fn name(&self) -> &str {
        name_of_type!(Defense)
    }

    fn execute(&mut self, ctx: &mut Context) -> Action {
        ctx.eeg.track(Event::Defense);

        // If we're not between the ball and our goal, get there.
        if !Self::is_between_ball_and_own_goal(ctx) {
            return Action::call(Retreat::new());
        }

        // If we're already in goal, try to take control of the ball somehow.
        if ctx.scenario.possession() < Scenario::POSSESSION_CONTESTABLE {
            ctx.eeg
                .log("[Defense] already in goal; going for a defensive hit");
            Action::call(Chain::new(Priority::Idle, vec![
                Box::new(FollowRoute::new(GroundIntercept::new())),
                Box::new(GroundedHit::hit_towards(defensive_hit)),
            ]))
        } else {
            Action::call(TepidHit::new())
        }
    }
}

pub struct PushToOwnCorner;

impl PushToOwnCorner {
    const MAX_BALL_Z: f32 = HitToOwnCorner::MAX_BALL_Z;

    pub fn new() -> Self {
        PushToOwnCorner
    }

    fn shot_angle(ball_loc: Point3<f32>, car_loc: Point3<f32>, aim_loc: Point2<f32>) -> f32 {
        let angle_me_ball = car_loc.coords.to_2d().angle_to(ball_loc.coords.to_2d());
        let angle_ball_goal = ball_loc.coords.to_2d().angle_to(aim_loc.coords);
        (angle_me_ball - angle_ball_goal).normalize_angle().abs()
    }

    fn goal_angle(ball_loc: Point3<f32>, goal: &Goal) -> f32 {
        let goal_to_ball_axis = (ball_loc.to_2d() - goal.center_2d).to_axis();
        goal_to_ball_axis.rotation_to(&goal.normal_2d).angle().abs()
    }
}

impl Behavior for PushToOwnCorner {
    fn name(&self) -> &str {
        name_of_type!(PushToOwnCorner)
    }

    fn execute(&mut self, ctx: &mut Context) -> Action {
        let impending_concede_soon = ctx
            .scenario
            .impending_concede()
            .map(|f| f.t < 5.0)
            .unwrap_or_default();

        let me_intercept =
            naive_ground_intercept_2(&ctx.me().into(), ctx.scenario.ball_prediction(), |ball| {
                ball.loc.z < Self::MAX_BALL_Z
            });

        let enemy_shootable_intercept = ctx
            .enemy_cars()
            .filter_map(|enemy| {
                naive_ground_intercept_2(&enemy.into(), ctx.scenario.ball_prediction(), |ball| {
                    let own_goal = ctx.game.own_goal().center_2d;
                    ball.loc.z < GroundedHit::max_ball_z()
                        && Self::shot_angle(ball.loc, enemy.Physics.loc(), own_goal) < PI / 3.0
                        && Self::goal_angle(ball.loc, ctx.game.own_goal()) < PI / 3.0
                })
            })
            .min_by_key(|i| NotNan::new(i.time).unwrap());

        if let Some(ref i) = me_intercept {
            ctx.eeg
                .log(format!("[Defense] me_intercept: {:.2}", i.time));
            ctx.eeg.draw(Drawable::GhostBall(
                i.ball_loc,
                color::for_team(ctx.game.team),
            ));
        }
        if let Some(ref i) = enemy_shootable_intercept {
            ctx.eeg
                .log(format!("[Defense] enemy_shoot_intercept: {:.2}", i.time));
            ctx.eeg.draw(Drawable::GhostBall(
                i.ball_loc,
                color::for_team(ctx.game.enemy_team),
            ));
        }

        match (me_intercept, enemy_shootable_intercept) {
            (_, None) => {
                if !impending_concede_soon {
                    ctx.eeg.log("Safe for now");
                    Action::Return
                } else {
                    ctx.eeg.log("Hitting away from goal");
                    Action::call(HitToOwnCorner::new())
                }
            }
            (None, _) => {
                ctx.eeg.log("Can't reach ball");
                Action::Abort
            }
            (Some(_), Some(_)) => {
                if ctx.scenario.possession() >= 3.0 {
                    ctx.eeg.log("we have all the time in the world");
                    Action::Abort
                } else if ctx.scenario.possession() >= Scenario::POSSESSION_CONTESTABLE {
                    ctx.eeg.log("Swatting ball away from enemy");
                    Action::call(HitToOwnCorner::new())
                } else if ctx.scenario.possession() >= -Scenario::POSSESSION_CONTESTABLE {
                    ctx.eeg.log("Defensive race");
                    Action::call(HitToOwnCorner::new())
                } else {
                    ctx.eeg.log("Can't reach ball before enemy");
                    Action::Abort
                }
            }
        }
    }
}

pub struct HitToOwnCorner;

impl HitToOwnCorner {
    const MAX_BALL_Z: f32 = BounceShot::MAX_BALL_Z;

    pub fn new() -> Self {
        HitToOwnCorner
    }
}

impl Behavior for HitToOwnCorner {
    fn name(&self) -> &str {
        name_of_type!(HitToOwnCorner)
    }

    fn execute(&mut self, ctx: &mut Context) -> Action {
        ctx.eeg.track(Event::HitToOwnCorner);

        Action::call(Chain::new(Priority::Striking, vec![
            Box::new(FollowRoute::new(GroundIntercept::new())),
            Box::new(GroundedHit::hit_towards(Self::aim)),
        ]))
    }
}

impl HitToOwnCorner {
    fn aim(ctx: &mut GroundedHitAimContext) -> Result<GroundedHitTarget, ()> {
        let avoid = ctx.game.own_goal().center_2d;

        let me_loc = ctx.car.Physics.loc_2d();
        let ball_loc = ctx.intercept_ball_loc.to_2d();
        let me_to_ball = ball_loc - me_loc;

        let ltr_dir = Rotation2::new(PI / 6.0) * me_to_ball;
        let ltr = WallRayCalculator::calculate(ball_loc, ball_loc + ltr_dir);
        let rtl_dir = Rotation2::new(-PI / 6.0) * me_to_ball;
        let rtl = WallRayCalculator::calculate(ball_loc, ball_loc + rtl_dir);

        let result = if (avoid - ltr).norm() > (avoid - rtl).norm() {
            ctx.eeg.log("push from left to right");
            ltr
        } else {
            ctx.eeg.log("push from right to left");
            rtl
        };

        match WallRayCalculator::wall_for_point(ctx.game, result) {
            Wall::OwnGoal => {
                ctx.eeg.log("avoiding the own goal");
                Err(())
            }
            _ => Ok(GroundedHitTarget::new(
                ctx.intercept_time,
                GroundedHitTargetAdjust::RoughAim,
                result,
            )),
        }
    }
}

/// For `GroundedHit::hit_towards`, calculate an aim location which puts us
/// between the ball and our own goal.
pub fn defensive_hit(ctx: &mut GroundedHitAimContext) -> Result<GroundedHitTarget, ()> {
    let target_angle = blocking_angle(
        ctx.intercept_ball_loc.to_2d(),
        ctx.car.Physics.loc_2d(),
        ctx.game.own_goal().center_2d,
        PI / 6.0,
    );
    let aim_loc = ctx.intercept_ball_loc.to_2d() - Vector2::unit(target_angle) * 4000.0;
    let dist_defense = (ctx.game.own_goal().center_2d - ctx.car.Physics.loc_2d()).norm();
    let defense_angle = (ctx.intercept_ball_loc.to_2d() - ctx.game.own_goal().center_2d)
        .rotation_to(ctx.intercept_ball_loc.to_2d() - ctx.car.Physics.loc_2d());
    let adjust = if dist_defense < 2500.0 && defense_angle.angle().abs() < PI / 3.0 {
        GroundedHitTargetAdjust::StraightOn
    } else {
        GroundedHitTargetAdjust::RoughAim
    };
    Ok(GroundedHitTarget::new(ctx.intercept_time, adjust, aim_loc))
}

/// Calculate an angle from `ball_loc` to `car_loc`, trying to get between
/// `ball_loc` and `block_loc`, but not adjusting the approach angle by more
/// than `max_angle_diff`.
fn blocking_angle(
    ball_loc: Point2<f32>,
    car_loc: Point2<f32>,
    block_loc: Point2<f32>,
    max_angle_diff: f32,
) -> f32 {
    let naive_angle = ball_loc.coords.angle_to(car_loc.coords);
    let block_angle = ball_loc.coords.angle_to(block_loc.coords);
    let adjust = (block_angle - naive_angle)
        .normalize_angle()
        .max(-max_angle_diff)
        .min(max_angle_diff);
    (naive_angle + adjust).normalize_angle()
}

#[cfg(test)]
mod integration_tests {
    use crate::{
        behavior::defense::{defense::HitToOwnCorner, Defense},
        eeg::Event,
        integration_tests::helpers::{TestRunner, TestScenario},
        strategy::Runner,
    };
    use brain_test_data::recordings;
    use common::prelude::*;
    use nalgebra::{Point2, Point3, Rotation3, Vector3};

    #[test]
    fn bouncing_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-3143.9788, -241.96017, 1023.1816),
                ball_vel: Vector3::new(717.56323, -1200.3536, 331.91443),
                car_loc: Point3::new(-4009.9998, -465.8022, 86.914),
                car_rot: Rotation3::from_unreal_angles(-0.629795, -0.7865487, 0.5246214),
                car_vel: Vector3::new(982.8443, -1059.1908, -935.80194),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run();

        let start_time = test.sniff_packet().GameInfo.TimeSeconds;

        let mut max_z = 0.0_f32;
        loop {
            let packet = test.sniff_packet();
            let elapsed = packet.GameInfo.TimeSeconds - start_time;
            if elapsed >= 4.0 {
                break;
            }
            if elapsed >= 1.0 && packet.GameBall.Physics.Velocity.Z > 0.0 {
                max_z = max_z.max(packet.GameBall.Physics.Location.Z);
            }
        }

        test.examine_events(|events| {
            assert!(events.contains(&Event::Defense));
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Location);
        assert!(packet.GameBall.Physics.Location.X >= 800.0);
        assert!(packet.GameBall.Physics.Location.Y >= -4000.0);

        // Should power-shot, meaning the ball bounces high.
        assert!(max_z >= 500.0, "{}", max_z);
    }

    #[test]
    fn redirect_away_from_goal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2667.985, 779.3049, 186.92154),
                ball_vel: Vector3::new(760.02606, -1394.5569, -368.39642),
                car_loc: Point3::new(-2920.1282, 1346.1251, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.1758921, 0.0),
                car_vel: Vector3::new(688.0767, -1651.0865, 8.181303),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(100);

        // This result is just *okay*
        test.examine_events(|events| {
            assert!(events.contains(&Event::Defense));
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });
    }

    #[test]
    #[ignore] // TODO
    fn last_second_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-1150.811, -1606.0569, 102.36157),
                ball_vel: Vector3::new(484.87906, -1624.8169, 32.10115),
                car_loc: Point3::new(-1596.7955, -1039.2034, 17.0),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.4007162, 0.0000958738),
                car_vel: Vector3::new(242.38637, -1733.6719, 8.41),
                boost: 0,
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(3000);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn slow_bouncer() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2849.355, -2856.8281, 1293.4608),
                ball_vel: Vector3::new(907.1093, -600.48956, 267.59674),
                car_loc: Point3::new(1012.88916, -3626.2666, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -0.8467574, 0.0),
                car_vel: Vector3::new(131.446, -188.83897, 8.33),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x < -2000.0);
        assert!(packet.GameBall.Physics.vel().x < -1000.0);
    }

    #[test]
    #[ignore(note = "The great bankruptcy of 2018")]
    fn falling_save_from_the_side() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2353.9868, -5024.7144, 236.38712),
                ball_vel: Vector3::new(-1114.3461, 32.5409, 897.3589),
                car_loc: Point3::new(2907.8083, -4751.0806, 17.010809),
                car_rot: Rotation3::from_unreal_angles(-0.018216021, -2.7451544, -0.0073822825),
                car_vel: Vector3::new(-1412.7858, -672.18933, -6.2963967),
                boost: 0,
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(3000);

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x < -1200.0);
        assert!(packet.GameBall.Physics.vel().y > 500.0);
    }

    #[test]
    fn retreating_push_to_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(436.92395, 1428.1085, 93.15),
                ball_vel: Vector3::new(-112.55582, -978.27814, 0.0),
                car_loc: Point3::new(1105.1365, 2072.0022, 17.0),
                car_rot: Rotation3::from_unreal_angles(-0.009491506, -2.061095, -0.0000958738),
                car_vel: Vector3::new(-546.6459, -1095.6816, 8.29),
                ..Default::default()
            })
            .behavior(Defense::new())
            .run_for_millis(1500);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 1500.0);
    }

    #[test]
    #[ignore] // TODO
    fn retreating_push_to_corner_from_awkward_side() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1948.3385, 1729.5826, 97.89405),
                ball_vel: Vector3::new(185.58005, -1414.3043, -5.051092),
                car_loc: Point3::new(896.22095, 1962.7969, 15.68419),
                car_rot: Rotation3::from_unreal_angles(-0.0131347105, -2.0592732, -0.010450244),
                car_vel: Vector3::new(-660.1856, -1449.2916, -3.7354965),
                ..Default::default()
            })
            .behavior(Defense::new())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    #[ignore] // TODO
    fn retreating_push_to_corner_from_awkward_angle() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2365.654, -86.64402, 114.0818),
                ball_vel: Vector3::new(988.47064, -1082.8477, -115.50357),
                car_loc: Point3::new(-2708.0007, -17.896847, 250.98781),
                car_rot: Rotation3::from_unreal_angles(0.28522456, -0.8319928, -0.05263472),
                car_vel: Vector3::new(550.82794, -1164.1539, 277.63806),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromLeftToRight));
            assert!(!events.contains(&Event::PushFromRightToLeft));
        });

        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.Velocity);
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    #[ignore(note = "The great bankruptcy of 2018")]
    fn push_from_corner_to_corner() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1620.9868, -4204.8145, 93.14),
                ball_vel: Vector3::new(-105.58675, 298.33023, 0.0),
                car_loc: Point3::new(3361.587, -4268.589, 16.258373),
                car_rot: Rotation3::from_unreal_angles(-0.0066152923, 1.5453898, -0.005752428),
                car_vel: Vector3::new(89.86856, 1188.811, 7.4339933),
                ..Default::default()
            })
            .behavior(HitToOwnCorner::new())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    #[ignore] // TODO
    fn push_from_corner_to_corner_2() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2517.809, -4768.475, 93.13),
                ball_vel: Vector3::new(-318.6226, 490.17892, 0.0),
                car_loc: Point3::new(3742.2703, -3277.4558, 16.954643),
                car_rot: Rotation3::from_unreal_angles(-0.009108011, 2.528288, -0.0015339808),
                car_vel: Vector3::new(-462.4023, 288.65112, 9.278907),
                boost: 10,
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(2000);

        test.sleep_millis(2000);
        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().norm() >= 2000.0);
    }

    #[test]
    fn same_side_corner_push() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2545.9438, -4174.64, 318.26862),
                ball_vel: Vector3::new(985.6374, -479.52872, -236.39767),
                car_loc: Point3::new(-1808.3466, -3266.7039, 16.41444),
                car_rot: Rotation3::from_unreal_angles(-0.009203885, -0.65855706, -0.0015339808),
                car_vel: Vector3::new(947.339, -565.98175, 15.669456),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(2000);

        test.examine_events(|events| {
            assert!(events.contains(&Event::HitToOwnCorner));
            assert!(events.contains(&Event::PushFromRightToLeft));
            assert!(!events.contains(&Event::PushFromLeftToRight));
        });
        let packet = test.sniff_packet();
        println!("{:?}", packet.GameBall.Physics.vel());
        assert!(packet.GameBall.Physics.vel().x < -300.0);
    }

    #[test]
    #[ignore] // TODO
    fn slow_rolling_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1455.9731, -4179.0796, 93.15),
                ball_vel: Vector3::new(-474.48724, -247.0518, 0.0),
                car_loc: Point3::new(2522.638, -708.08484, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, 2.6835077, 0.0),
                car_vel: Vector3::new(-1433.151, 800.56586, 8.33),
                boost: 0,
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(5000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.vel().x < -1000.0);
    }

    #[test]
    fn slow_retreating_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(1446.3031, -2056.4917, 213.57251),
                ball_vel: Vector3::new(-1024.0333, -1593.1566, -244.15135),
                car_loc: Point3::new(314.3022, -1980.4884, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.7653242, 0.0),
                car_vel: Vector3::new(-268.87683, -1383.9724, 8.309999),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(2000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x >= 1000.0);
        assert!(packet.GameBall.Physics.vel().x >= 500.0);
    }

    #[test]
    fn fast_retreating_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(63.619453, -336.2556, 93.03),
                ball_vel: Vector3::new(-189.17311, -1918.067, 0.0),
                car_loc: Point3::new(-103.64991, 955.411, 16.99),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.5927514, 0.0),
                car_vel: Vector3::new(-57.26778, -2296.9263, 8.53),
                ..Default::default()
            })
            .behavior(Runner::soccar())
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.loc().x < 1000.0);
        assert!(packet.GameBall.Physics.vel().x < 500.0);
    }

    #[test]
    fn jump_save_from_inside_goal() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::JUMP_SAVE_FROM_INSIDE_GOAL, 106.0)
            .starting_boost(0.0)
            .behavior(Runner::soccar())
            .run();
        test.sleep_millis(3000);
        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore(note = "The great bankruptcy of 2018")]
    fn retreat_then_save() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(-2503.1099, -3172.46, 92.65),
                ball_vel: Vector3::new(796.011, -1343.8209, 0.0),
                car_loc: Point3::new(-3309.3298, -1332.26, 17.01),
                car_rot: Rotation3::from_unreal_angles(0.009505707, -0.79850733, -0.000105084495),
                car_vel: Vector3::new(543.18097, -569.061, 8.321),
                ..Default::default()
            })
            .starting_boost(0.0)
            .behavior(Runner::soccar())
            .run_for_millis(6000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Location.X < -1000.0);
        assert!(!test.enemy_has_scored());
    }

    #[test]
    #[ignore(note = "The great bankruptcy of 2018")]
    fn clear_around_goal_wall() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::CLEAR_AROUND_GOAL_WALL, 327.0)
            .starting_boost(100.0)
            .behavior(Runner::soccar())
            .run();
        test.sleep_millis(3000);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Location.X < -1000.0);
        assert!(packet.GameBall.Physics.Velocity.X < -100.0);
        assert!(!test.enemy_has_scored());
    }

    /// This guards against a behavior where even a tiny touch by the enemy
    /// triggers SameBallTrajectory and causes us to turn around and retreat
    /// back to goal.
    #[test]
    fn defensive_confidence() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::DEFENSIVE_CONFIDENCE, 24.0)
            .starting_boost(65.0)
            .behavior(Runner::soccar())
            .run_for_millis(3500);

        let packet = test.sniff_packet();
        assert!(packet.GameBall.Physics.Velocity.Y >= 500.0);
    }

    #[test]
    fn do_not_own_goal() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                ball_loc: Point3::new(2972.65, -4341.88, 1418.28),
                ball_vel: Vector3::new(-1411.2909, 212.371, 486.57098),
                car_loc: Point3::new(-2043.4099, -1165.84, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009681773, -0.7725685, 0.00012306236),
                car_vel: Vector3::new(1125.0809, -1248.741, 8.311),
                ..Default::default()
            })
            .starting_boost(10.0)
            .behavior(Runner::soccar())
            .run_for_millis(4000);

        assert!(!test.enemy_has_scored());
        let packet = test.sniff_packet();
        let ball_loc = packet.GameBall.Physics.loc_2d();
        // Sometimes enemy_has_scored doesn't work since the framework doesn't support
        // it. Also make sure there wasn't a goal reset.
        assert!((ball_loc - Point2::origin()).norm() >= 1.0);
    }

    #[test]
    fn low_boost_block_goal() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::BLOCK_GOAL_WITH_NO_BOOST, 61.5)
            .starting_boost(0.0)
            .enemy_starting_boost(50.0)
            .behavior(Runner::soccar())
            .run_for_millis(2500);

        assert!(!test.enemy_has_scored());
    }

    #[test]
    fn inconvenient_angle_hit_to_the_side() {
        let test = TestRunner::new()
            .one_v_one(&*recordings::INCONVENIENT_ANGLE_HIT_TO_THE_SIDE, 419.5)
            .starting_boost(0.0)
            .enemy_starting_boost(0.0)
            .behavior(Runner::soccar())
            .run_for_millis(5000);

        assert!(!test.enemy_has_scored());
    }
}