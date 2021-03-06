use crate::routing::{
    models::{CarState, RoutePlanner},
    plan::{
        ground_straight::GroundStraightPlanner, ground_turn::PathingUnawareTurnPlanner,
        higher_order::ChainedPlanner,
    },
    segments::StraightMode,
};
use common::{physics, prelude::*, rl};
use nalgebra::Point2;

/// Calculate whether driving straight to `target_loc` would intersect the goal
/// wall. If so, return the route we should follow to get outside the goal.
pub fn avoid_plowing_into_goal_wall(
    start: &CarState,
    target_loc: Point2<f32>,
) -> Option<Box<dyn RoutePlanner>> {
    let waypoint = avoid_goal_wall_waypoint(start, target_loc)?;
    Some(Box::new(ChainedPlanner::chain(vec![
        Box::new(PathingUnawareTurnPlanner::new(waypoint, None)),
        Box::new(GroundStraightPlanner::new(waypoint, StraightMode::Asap)
            // The idea is – turning is harder when you're going faster, and the
            // turn around the post is an important one, so let's make the turn
            // as easy as we can.
            //
            // I'd much rather have said something like `.max_speed(1000)` or
            // something, but this was easier.
            .allow_boost(false)),
    ])))
}

/// Calculate whether driving straight to `target_loc` would intersect the goal
/// wall. If so, return the waypoint we should drive to first to avoid
/// embarrassing ourselves.
#[allow(clippy::float_cmp)]
pub fn avoid_goal_wall_waypoint(start: &CarState, target_loc: Point2<f32>) -> Option<Point2<f32>> {
    let margin = 125.0;

    // Only proceed if we're crossing over the goalline.
    let brink = rl::FIELD_MAX_Y * start.loc.y.signum();
    if (brink - start.loc.y).signum() == (brink - target_loc.y).signum() {
        return None;
    }

    // Detect the degenerate state where we're starting outside the field. Add a
    // buffer zone since the routing before this point might have been a little
    // sloppy and put us in a not-so-precise location.
    if start.loc.x.abs() >= rl::GOALPOST_X + 200.0 {
        log::warn!("avoid_goal_wall_waypoint: starting position outside field?");
        return None;
    }

    let brink = (rl::FIELD_MAX_Y - 50.0) * start.loc.y.signum();
    let ray = physics::car_forward_axis_2d(start.rot.to_2d());
    let toi = (brink - start.loc.y) / ray.y;
    let cross_x = start.loc.x + toi * ray.x;
    if cross_x.abs() >= rl::GOALPOST_X - margin {
        Some(Point2::new(
            (rl::GOALPOST_X - margin) * cross_x.signum(),
            (rl::FIELD_MAX_Y - margin) * start.loc.y.signum(),
        ))
    } else {
        None
    }
}
