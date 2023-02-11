use bevy::app::AppExit;
use bevy::prelude::*;
use bevy_promise::prelude::*;
use bevy_promise_core::PromisesExtension;

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
        Promise::start(asyn!(s, time: Res<Time> => {
            let t = time.elapsed_seconds();
            info!("start with 31, started at {t}, start time stored in state.");
            s.map(|_| t).ok(31)
        }))
        .ok_then(asyn!(s, r => {
            info!("Continue first time with result: {r}, incrementing");
            s.ok(r + 1)
        }))
        .ok_then(asyn!(s, r => {
            info!("Continue second time with result: {r}");
            if r > 31 {
                s.reject(format!("{r} actually more then 4-bit"))
            } else {
                s.resolve(r + 1)
            }
        }))
        .or_else(asyn!(s, e => {
            info!("Looks like smth wrong: {e}");
            s.ok(31)
        }))
        .ok_then(asyn!(s, r => {
            info!("continue third time with result: {r}");
            s.asyn().timeout(1.5).with_ok(r + 1)
        }))
        .ok_then(asyn!(s, r => {
            info!("continue after 1.5 sec delay with {r}");
            s.asyn().timeout(1.5)
        }))
        .ok_then(asyn!(s, _, mut commands: Commands => {
            info!("complete after 1.5 sec delay, adding custom command");
            commands.add(|_: &mut World| info!("Executing custom command at the end."));
            let timeout = rand();
            info!("Requesting https:://google.com with timeout {timeout:0.2}s");
            s.any((
                // wait for first completed promise
                asyn::timeout(timeout),
                asyn::http::get("https://google.com").send(),
            ))
        }))
        .ok_then(asyn!(s, (timeout, response) => {
            if timeout.is_some() {
                info!("Request timed out");
            } else {
                match response.unwrap() {
                    Ok(r) => info!("Respond faster then timeout with {}", r.status),
                    Err(e) => info!("Respond faster then timeout with error: {e}"),
                }
            }
            s.ok(())
        }))
        .ok_then(asyn!(s, _ => {
            s.all((
                asyn::http::get("https://google.com").send(),
                asyn::http::get("https://bevyengine.org").send(),
            ))
        }))
        .ok_then(asyn!(s, r => {
            let (google, bevy) = r;
            if let Ok(google) = google {
                info!("Google respond with {}", google.status);
            } else {
                info!("Google respond error");
            }
            if let Ok(bevy) = bevy {
                info!("Bevy respond with {}", bevy.status);
            } else {
                info!("Bevy respond error");
            }
            s.ok(())
        }))
        .ok_then(asyn!(s, _ => {
            info!("Requesting any");
            ["https://google.com", "https://bevyengine.org", "https://github.com"]
                .iter()
                .map(|url| {
                    info!("  {url}");
                    asyn::http::get(url).send().with(*url)
                })
                .promise()
                .any()
                .with(s.value)
        }))
        .ok_then(asyn!(s, (url, result) => {
            let resp = match result {
                Ok(r) => format!("{}", r.status),
                Err(e) => e,
            };
            info!("{url} respond faster then others with {resp}");
            s.ok(())
        }))
        .ok_then(asyn!(s, _ => {
            info!("Requesting all");
            ["https://google.com", "https://bevyengine.org", "https://github.com"]
                .iter()
                .map(|url| {
                    info!("  {url}");
                    url
                })
                .map(|url| asyn::http::get(url).send().with(*url))
                .promise()
                .all()
                .with(s.value)
        }))
        .ok_then(asyn!(s, r => {
            info!("Services responded:");
            for r in r.iter() {
                match r {
                    Ok(r) => info!("  {}", r.status),
                    Err(e) => info!("  {e}"),
                }
            }
            s.ok(())
        }))
        .ok_then(asyn!(s, _ => {
            info!("requesing https://bevyengine.org");
            s.asyn().http().get("https://bevyengine.org")
        }))
        .then(asyn!(s, r => {
            match r {
                Ok(r) => info!("Bevy respond with {}, body size: {}", r.status, r.bytes.len()),
                Err(e) => warn!("Error requesting Bevy: {e}"),
            }
            s.then(log_request("https://google.com")).ok_then(asyn!(|s, r| {
                info!("Request done in {r} secs");
                s.ok(())
            }))
        }))
        .ok_then(asyn!(s, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
            info!(
                "Done, time to process: {} (start time took from state {}",
                time.elapsed_seconds() - s.value,
                s
            );
            exit.send(AppExit);
            s.ok(())
        })),
    );
}

/// Returns a promise that requests `url`, logs the process
/// and resolves with seconds spent to complete requests as `f32`
fn log_request(url: &'static str) -> Promise<f32, (), ()> {
    Promise::new(
        url,
        asyn!(|s, time: Res<Time>| {
            let url = s.value;
            let start = time.elapsed_seconds();
            info!("Requesting {url} at {start:0.2}");
            s.map(|url| (url, start)).asyn().http().get(url)
        }),
    )
    .then(asyn!(|s, r, time: Res<Time>| {
        match r {
            Ok(r) => info!("{} respond with {}, body size: {}", s.value.0, r.status, r.bytes.len()),
            Err(e) => warn!("Error requesting {}: {e}", s.value.0),
        }
        let duration = time.elapsed_seconds() - s.value.1;
        s.map(|_| ()).ok(duration)
    }))
}

// almost implemeted by chatgpt
pub fn rand() -> f32 {
    use std::hash::{Hash, Hasher};
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (epoch, pid).hash(&mut hasher);
    let seed = hasher.finish() as u64;
    (seed as f32) / u64::MAX as f32
}
