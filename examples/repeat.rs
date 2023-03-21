//! This example demonstrates how to use `Promise::repeat()`
//! to create async loops.

use bevy::prelude::*;
use pecs::prelude::*;
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PecsPlugin)
        .add_startup_system(setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.add(
        Promise::repeat(
            0,
            asyn!(state => {
                info!("#{}", state.value);
                state.value += 1;
                let iterations = state.value;
                state.asyn().timeout(1.).with_result(if iterations > 3 {
                    Repeat::Break("Done!")
                } else {
                    Repeat::Continue
                })
            }),
        )
        .then(asyn!(s, r => {
            info!("Repeated {} times, result: {}", s.value, r);
            s.pass()
        })),
    )
    // )
}
