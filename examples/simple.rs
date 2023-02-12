use bevy::{app::AppExit, prelude::*};
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
        Promise::start(asyn!(state, time: Res<Time> => {
            info!("Wait a second..");
            let started_at = time.elapsed_seconds();
            state.with(started_at).asyn().timeout(1.0)
        }))
        .then(asyn!(state, _ => {
            info!("How large is is the Bevy main web page?");
            state.asyn().http().get("https://bevyengine.org")
        }))
        .then(asyn!(state, result => {
            match result {
                Ok(response) => info!("It is {} bytes!", response.bytes.len()),
                Err(err) => info!("Ahhh... something goes wrong: {err}")
            }
            state.pass()
        }))
        .then(asyn!(state, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
            let duration = time.elapsed_seconds() - state.value;
            info!("It tooks {duration:0.2}s to do this job.");
            info!("Exiting now");
            exit.send(AppExit);
            state.pass()
        })),
    );
}