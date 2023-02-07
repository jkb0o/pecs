
use bevy::prelude::*;
use bevy_promise::AsyncValue;
use bevy_promise::AsyncState;
use bevy_promise::prelude::*;
use bevy_promise::timer::TimerOpsExtension;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PromisePlugin)
        .add_startup_system(setup)
        .run();
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());

    // commands.add(promise!(|c, time: Res<Time>| {
    //).then(promise!(|c, r| {
    // 
    //}))
    // }))
    commands.add(Promise::new(|In(AsyncState(s)), time: Res<Time>| {
        info!("start with 31, started at {}, start time stored in state.", time.elapsed_seconds());
        s.map(|_| time.elapsed_seconds()).ok(31)
    })
    .then(|In((AsyncState(s), AsyncValue(r)))| {
        info!("Continue first time with result: {r}, incrementing");
        s.ok(r+1)
    }).then(|In((AsyncState(s), AsyncValue(r)))| {
        info!("Continue second time with result: {r}");
        if r > 31 {
            s.reject(format!("{r} actually more then 4-bit"))
        } else {
            s.resolve(r+1)
        }
    }).catch(|In((AsyncState(s), AsyncValue(e)))| {
        info!("Looks like smth wrong: {e}");
        s.ok(31)
    }).then(|In((AsyncState(s), AsyncValue(r)))| {
        info!("continue third time with result: {r}");
        s.ops().timer().delay(1.5).result(r + 1)
    }).then(|In((AsyncState(s), AsyncValue(r)))| {
        info!("continue after 1.5 sec delay with {r}");
        s.ops().timer().delay(1.5)
    }).then(|In((AsyncState(s), _)), mut commands: Commands, time: Res<Time>| {
        info!("complete after 1.5 sec delay, time to process: {} (start time took from state {}", time.elapsed_seconds() - s.0, s);
        commands.add(|_: &mut World|{
            info!("Executing custom command at the end.")
        });
        s.ok(())
    }));
    // }));
}