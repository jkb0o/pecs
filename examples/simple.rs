//! This example shows how to sequentially call
//! promises by chaining them with `then` method.
use bevy::prelude::*;
use pecs::prelude::*;
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PecsPlugin)
        .add_startup_system(setup)
        .run();
}

fn setup(mut commands: Commands, time: Res<Time>) {
    let start = time.elapsed_seconds();
    // create PromiseLike chainable commands with the current time as the state
    commands
        .promise(|| start)
        // will be executed right after current stage
        .then(asyn!(state => {
            info!("Wait a second..");
            state.asyn().timeout(1.0)
        }))
        // will be executed in a second after the previous call
        .then(asyn!(state => {
            info!("How large is is the Bevy main web page?");
            state.asyn().http().get("https://bevyengine.org")
        }))
        // will be executed after we get the response/error
        .then(asyn!(state, result => {
            match result {
                Ok(response) => info!("It is {} bytes!", response.bytes.len()),
                Err(err) => info!("Ahhh... something goes wrong: {err}")
            }
            state.pass()
        }))
        // will be executed right after the previous one
        .then(asyn!(state, time: Res<Time> => {
            let duration = time.elapsed_seconds() - state.value;
            info!("It tooks {duration:0.2}s to do this job.");
            info!("Exiting now");
            asyn::app::exit()
        }));
}