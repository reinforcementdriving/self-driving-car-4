#[cfg(test)]
mod integration_tests {
    use behavior::RootBehavior;
    use collect::ExtendRotation3;
    use integration_tests::helpers::{TestRunner, TestScenario};
    use nalgebra::{Rotation3, Vector3};

    #[test]
    #[ignore] // TODO
    fn falling_in_front_of_far_corner() {
        let test = TestRunner::start(
            RootBehavior::new(),
            TestScenario {
                ball_loc: Vector3::new(882.9138, -5002.2944, 608.2664),
                ball_vel: Vector3::new(-211.04604, 37.17434, 459.58438),
                car_loc: Vector3::new(-2512.3357, -2450.706, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.009683254, -0.68204623, -0.0000958738),
                car_vel: Vector3::new(786.13666, -620.0981, 8.309999),
                ..Default::default()
            },
        );

        test.sleep_millis(5000);

        assert!(test.has_scored());
    }

    #[test]
    #[ignore] // TODO
    fn rolling_quickly() {
        let test = TestRunner::start(
            RootBehavior::new(),
            TestScenario {
                ball_loc: Vector3::new(2792.5564, 2459.176, 94.02834),
                ball_vel: Vector3::new(-467.7808, -2086.822, -88.445175),
                car_loc: Vector3::new(3001.808, 3554.98, 16.99),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.7710767, 0.0000958738),
                car_vel: Vector3::new(-379.28546, -1859.9683, 8.41),
                ..Default::default()
            },
        );

        test.sleep_millis(5000);

        assert!(test.has_scored());
    }

    #[test]
    #[ignore] // TODO
    fn rolling_around_corner_into_box() {
        let test = TestRunner::start(
            RootBehavior::new(),
            TestScenario {
                ball_loc: Vector3::new(3042.6016, -4141.044, 180.57321),
                ball_vel: Vector3::new(-1414.86847, -1357.0486, -0.0),
                car_loc: Vector3::new(720.54016, 635.665, 17.01),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.4134674, 0.0),
                car_vel: Vector3::new(256.23804, -1591.1218, 8.3),
                ..Default::default()
            },
        );

        test.sleep_millis(5000);

        assert!(test.has_scored());
    }

    #[test]
    #[ignore] // TODO
    fn low_bouncing_directly_ahead() {
        let test = TestRunner::start(
            RootBehavior::new(),
            TestScenario {
                ball_loc: Vector3::new(-916.57043, -5028.2397, 449.42386),
                ball_vel: Vector3::new(215.22325, 0.07279097, -403.102),
                car_loc: Vector3::new(-320.59094, -2705.4436, 17.02),
                car_rot: Rotation3::from_unreal_angles(-0.00958738, -1.6579456, 0.0),
                car_vel: Vector3::new(-85.847946, -990.35706, 8.0),
                ..Default::default()
            },
        );

        test.sleep_millis(5000);

        assert!(test.has_scored());
    }
}
