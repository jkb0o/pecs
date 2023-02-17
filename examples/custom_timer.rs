//! This Example shows how you can create custom promises
//! with `Promise::register()` method and resolve them from
//! you system with `commands.promise(id).resolve(result)`
use bevy::prelude::*;
use pecs::prelude::*;
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PecsPlugin)
        .add_system(process_timers_system)
        .add_startup_system(setup)
        .run();
}

#[derive(Component)]
/// Holds PromiseId and the time when the timer should time out.
pub struct MyTimer(PromiseId, f32);

/// creates promise that will resolve after [`duration`] seconds
pub fn delay(duration: f32) -> Promise<(), ()> {
    Promise::register(
        // this will be invoked when promise's turn comes
        move |world, id| {
            let now = world.resource::<Time>().elapsed_seconds();
            // store timer
            world.spawn(MyTimer(id, now + duration));
        },
        // this will be invoked when promise got discarded
        move |world, id| {
            let entity = {
                let mut timers = world.query::<(Entity, &MyTimer)>();
                timers
                    .iter(world)
                    .filter(|(_entity, timer)| timer.0 == id)
                    .map(|(entity, _timer)| entity)
                    .next()
            };
            if let Some(entity) = entity {
                world.despawn(entity);
            }
        },
    )
}

/// iterate ofver all timers and resolves completed
pub fn process_timers_system(timers: Query<(Entity, &MyTimer)>, mut commands: Commands, time: Res<Time>) {
    let now = time.elapsed_seconds();
    for (entity, timer) in timers.iter().filter(|(_, t)| t.1 < now) {
        let promise_id = timer.0;
        commands.promise(promise_id).resolve(());
        commands.entity(entity).despawn();
    }
}

fn setup(mut commands: Commands) {
    // `delay()` can be called from inside promise
    commands
        .promise(|| ())
        .then(asyn! {
            info!("Starting");
            delay(1.)
        })
        .then(asyn! {
            info!("Completing");
        });

    // or queued directly to Commands
    commands.promise(delay(2.)).then(asyn! {
        info!("I'm another timer");
        asyn::app::exit()
    });
}
