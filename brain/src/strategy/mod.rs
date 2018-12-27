pub use crate::strategy::{
    behavior::{Action, Behavior, Priority},
    context::Context,
    dropshot::Dropshot,
    game::{
        infer_game_mode, BoostPickup, Game, Goal, Team, Vehicle, SOCCAR_GOAL_BLUE,
        SOCCAR_GOAL_ORANGE,
    },
    runner::Runner,
    scenario::Scenario,
    soccar::Soccar,
};

mod behavior;
mod context;
mod dropshot;
mod game;
#[cfg(test)]
pub mod null;
mod runner;
mod scenario;
mod soccar;
#[allow(clippy::module_inception)]
mod strategy;
