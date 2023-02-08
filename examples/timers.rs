use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_promise::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PromisePlugin)
        .add_startup_system(setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.add(
        promise!(|s, time: Res<Time>| {
            let t = time.elapsed_seconds();
            info!(
                "start with 31, started at {}, start time stored in state.",
                t
            );
            s.with(|_| t).ok(31)
        })
        .then(promise!(|s, r| {
            info!("Continue first time with result: {r}, incrementing");
            s.ok(r + 1)
        }))
        .then(promise!(|s, r| {
            info!("Continue second time with result: {r}");
            if r > 31 {
                s.reject(format!("{r} actually more then 4-bit"))
            } else {
                s.resolve(r + 1)
            }
        }))
        .catch(promise!(|s, e| {
            info!("Looks like smth wrong: {e}");
            s.ok(31)
        }))
        .then(promise!(|s, r| {
            info!("continue third time with result: {r}");
            s.ops().timer().delay(1.5).result(r + 1)
        }))
        .then(promise!(|s, r| {
            info!("continue after 1.5 sec delay with {r}");
            s.ops().timer().delay(1.5)
        }))
        .then(promise!(|s, _, mut commands: Commands| {
            info!("complete after 1.5 sec delay, adding custom command");
            commands.add(|_: &mut World| info!("Executing custom command at the end."));
            s.ok(())
        }))
        .then(promise!(
            |s, _, time: Res<Time>, mut exit: EventWriter<AppExit>| {
                info!(
                    "Done, time to process: {} (start time took from state {}",
                    time.elapsed_seconds() - s.0,
                    s
                );
                exit.send(AppExit);
                s.ok(())
            }
        )),
    );
}
