use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_promise::http::{HttpOpsExtension, Response};
use bevy_promise::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PromisePlugin)
        .add_startup_system(setup)
        .run();
}

fn log_request(url: &'static str) -> Promise<f32, (), ()> {
    promise!(url => |s, time: Res<Time>| {
        let url = s.value;
        let start = time.elapsed_seconds();
        info!("Requesting {}", url);
        let r = s.with(|url| (url, start)).ops().http().get(url).send();
        r
    })
    .then_catch(promise!(|s as (_, _), r, time: Res<Time>| {
        match r as Result<Response, String> {
            Ok(r) => info!("{} respond with {}, body size: {}", s.value.0, r.status, r.bytes.len()),
            Err(e) => warn!("Error requesting {}: {e}", s.value.0)
        }
        let duration = time.elapsed_seconds() - s.value.1;
        s.with(|_|()).ok(duration)
    }))
}

fn setup(mut commands: Commands) {
    commands.spawn(Camera2dBundle::default());
    commands.add(
        promise!(|s, time: Res<Time>| {
            let t = time.elapsed_seconds();
            info!("start with 31, started at {t}, start time stored in state.");
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
            s.ops().timer().delay(1.5).map(move |_| r + 1)
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
        .then(promise!(|s, _| {
            info!("requesing https://bevyengine.org");
            s.ops().http().get("https://bevyengine.org").send()
        }))
        .then_catch(promise!(|s, r| {
            match r as Result<Response, _> {
                Ok(r) => info!(
                    "Bevy respond with {}, body size: {}",
                    r.status,
                    r.bytes.len()
                ),
                Err(e) => warn!("Error requesting Bevy: {e}"),
            }
            s.then(log_request("https://google.com".into()))
                .then(promise!(|s, r| {
                    info!("Request done in {r} secs");
                    s.ok(())
                }))
        }))
        .then(promise!(
            |s, _, time: Res<Time>, mut exit: EventWriter<AppExit>| {
                info!(
                    "Done, time to process: {} (start time took from state {}",
                    time.elapsed_seconds() - s.value,
                    s
                );
                exit.send(AppExit);
                s.ok(())
            }
        )),
    );
}
