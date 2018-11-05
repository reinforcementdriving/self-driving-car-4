use common::prelude::*;
use maneuvers::GroundedHit;
use predict::naive_ground_intercept;
use routing::{
    models::{PlanningContext, RoutePlan, RoutePlanError, RoutePlanner},
    plan::{ground_straight::GroundStraightPlanner, ground_turn::TurnPlanner},
    recover::{IsSkidding, NotOnFlatGround},
    segments::StraightMode,
};

#[derive(Clone, new)]
pub struct GroundIntercept;

impl RoutePlanner for GroundIntercept {
    fn name(&self) -> &'static str {
        stringify!(GroundIntercept)
    }

    fn plan(&self, ctx: &PlanningContext) -> Result<RoutePlan, RoutePlanError> {
        guard!(
            ctx.start,
            NotOnFlatGround,
            RoutePlanError::MustBeOnFlatGround,
        );

        // Naive first pass to get a rough location.
        let guess = naive_ground_intercept(
            ctx.ball_prediction.iter(),
            ctx.start.loc,
            ctx.start.vel,
            ctx.start.boost,
            |ball| ball.loc.z < GroundedHit::max_ball_z() && ball.vel.z < 25.0,
        )
        .ok_or_else(|| RoutePlanError::UnknownIntercept)?;

        guard!(
            ctx.start,
            IsSkidding,
            RoutePlanError::MustNotBeSkidding {
                recover_target_loc: guess.car_loc.to_2d(),
            },
        );

        TurnPlanner::new(
            guess.ball_loc.to_2d(),
            Some(Box::new(GroundInterceptStraight::new())),
        )
        .plan(ctx)
    }
}

#[derive(Clone, new)]
struct GroundInterceptStraight;

impl RoutePlanner for GroundInterceptStraight {
    fn name(&self) -> &'static str {
        stringify!(GroundInterceptStraight)
    }

    fn plan(&self, ctx: &PlanningContext) -> Result<RoutePlan, RoutePlanError> {
        guard!(
            ctx.start,
            NotOnFlatGround,
            RoutePlanError::MustBeOnFlatGround,
        );

        let guess = naive_ground_intercept(
            ctx.ball_prediction.iter(),
            ctx.start.loc,
            ctx.start.vel,
            ctx.start.boost,
            |ball| ball.loc.z < GroundedHit::max_ball_z() && ball.vel.z < 25.0,
        )
        .ok_or_else(|| RoutePlanError::UnknownIntercept)?;

        guard!(
            ctx.start,
            IsSkidding,
            RoutePlanError::MustNotBeSkidding {
                recover_target_loc: guess.car_loc.to_2d(),
            },
        );

        let end_chop = 0.5;
        GroundStraightPlanner::new(
            guess.car_loc.to_2d(),
            guess.time,
            end_chop,
            StraightMode::Fake,
        )
        .plan(ctx)
    }
}
