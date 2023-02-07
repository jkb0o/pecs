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

    commands.add(Promise::new(|time: Res<Time>| {
        info!("start with 23, started at {}", time.elapsed_seconds());
        Promise::Ok(32)
    }).then(|In(r)| {
        info!("continue first time with {r}");
        if r > 31 {
            Promise::Reject("It actuall more then 4-bit")
        } else {
            Promise::Resolve(r+1)
        }
    }).catch(|In(e)| {
        info!("Looks like smth wrong: {e}");
        Promise::Resolve(31)
    }).then(|In(r)| {
        info!("continue second time with {r}");
        Async::timer().delay(1.5).result(r+1)
    // }).catch(|| {
    //     Promise::Ok(2)
    }).then(|In(r), time: Res<Time>| {
        info!("continue after 1.5 sec delay with {r}, now: {}", time.elapsed_seconds());
        Async::timer().delay(1.5)
    }).then(|mut commands: Commands| {
        info!("complete after 1.5");
        commands.add(|_: &mut World|{
            info!("Executing custom command at the end.")
        });
        Promise::Ok(())
    }));
}