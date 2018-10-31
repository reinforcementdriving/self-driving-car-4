use common::prelude::*;
use integration_tests::helpers::{TestRunner, TestScenario};
use nalgebra::{Rotation3, Vector3};
use strategy::Runner2;

#[test]
fn clear_ball_out_of_goal() {
    let test = TestRunner::start0(TestScenario {
        ball_loc: Vector3::new(-500.0, -5000.0, 0.0),
        car_loc: Vector3::new(500.0, -5000.0, 17.0),
        car_rot: Rotation3::from_unreal_angles(0.0, 210_f32.to_radians(), 0.0),
        ..Default::default()
    });
    test.set_behavior(Runner2::new());
    test.sleep_millis(2000);

    let packet = test.sniff_packet();
    assert!(packet.GameBall.Physics.Location.X < -1000.0);
}

#[test]
#[ignore]
// We came in too hot, we should be able to smack it to the side.
fn todo() {
    let test = TestRunner::start0(TestScenario {
        ball_loc: Vector3::new(-1004.2267, -1863.0571, 93.15),
        ball_vel: Vector3::new(1196.1945, -1186.7386, 0.0),
        car_loc: Vector3::new(1692.9968, -2508.7695, 17.01),
        car_rot: Rotation3::from_unreal_angles(-0.009779127, -2.0910075, 0.0),
        car_vel: Vector3::new(-896.0074, -1726.876, 8.375226),
        ..Default::default()
    });
    test.set_behavior(Runner2::new());
    unimplemented!();
}

#[allow(deprecated)]
mod template {
    use integration_tests::helpers::{TestRunner, TestScenario};
    use nalgebra::Vector3;
    use strategy::Runner2;

    #[test]
    #[ignore]
    fn scenario_template() {
        let test = TestRunner::new()
            .scenario(TestScenario {
                enemy_loc: Vector3::new(6000.0, 6000.0, 0.0),
                ..TestScenario::from_recorded_row("../logs/play.csv", 10.0)
            })
            .starting_boost(25.0)
            .behavior(Runner2::new())
            .run();
        test.sleep_millis(7000);
        unimplemented!();
    }

    #[test]
    #[ignore]
    fn recording_template() {
        let test = TestRunner::new()
            .preview_recording("../logs/play.csv", 100.0, 102.0, 105.0)
            .starting_boost(25.0)
            .behavior(Runner2::new())
            .run();
        test.sleep_millis(7_000);
        unimplemented!();
    }
}
