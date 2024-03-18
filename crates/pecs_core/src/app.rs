use bevy::app::AppExit;

use super::*;

pub fn exit() -> Promise<(), ()> {
    Promise::register(
        |world, _| {
            world.resource_mut::<Events<AppExit>>().send(AppExit);
        },
        // can't discard AppExit
        |_, _| {},
    )
}
