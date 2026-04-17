use bevy::prelude::*;

use crate::{Promise, PromiseId};

#[derive(Resource, Default)]
pub struct ExitRequested(pub bool);

pub fn exit() -> Promise<(), ()> {
    Promise::register(
        |world: &mut World, _: PromiseId| {
            world.insert_resource(ExitRequested(true));
        },
        |_, _| {},
    )
}

pub fn process_exit_request(mut exit_requested: ResMut<ExitRequested>, mut messages: ResMut<Messages<AppExit>>) {
    if exit_requested.0 {
        exit_requested.0 = false;
        messages.write(AppExit::Success);
    }
}
