//! This example demonstrates how to use `any()`/`all()`
//! in different ways for combining vector of promises
//! and react to result when all/any of the passed
//! promises got resolved.
use bevy::prelude::*;
use pecs::prelude::*;
fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PecsPlugin)
        .add_startup_system(setup)
        .run();
}

const URLS: &[&'static str] = &["https://google.com", "https://bevyengine.org", "https://github.com"];

fn setup(mut commands: Commands) {
    commands.add(
        // This is how Promise::all works in general:
        Promise::start(asyn!(_ => {
            info!("Requesting all {} urls", URLS.len());
            // requests is a vec of promises
            // requests: Vec<Promise<&&str, Result<Response, String>>>
            // &str is the state (came from .with(url) call)
            // Result<Response, String> is the http response/error
            let requests = URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send().with(url))
                .collect::<Vec<_>>();

            // Promise::all() takes a vec of promises and returns
            // other promise that will be resolved when all promises
            // from vec got resolved.
            Promise::all(requests)
        }))
        // It will be resolved with Vec<(&&str, Result<Response, String>)>:
        // the vector of (state, result) from promises you did
        // pass to Promise::all() in previous call
        .then(asyn!(_, resolved => {
            info!("Responses:");
            for (url, result) in resolved.iter() {
                match result {
                    Ok(response) => info!("  [{}] {url}", response.status),
                    Err(error) => info!("  Can't get {url}: {error}")
                }
            }
            Promise::pass()
        }))
        // Promise::any() works the same way: it takes a vec of promises
        // and returns another promise that will be resolved when the ANY
        // of provided promises got resolved:
        .then(asyn!({
            info!("Requesting any of {} urls", URLS.len());
            let requests = URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send().with(url))
                .collect::<Vec<_>>();
            Promise::any(requests)
        }))
        // resolved here is (&&str, Result<Response, String),
        // the result of first reolved promise passed to
        // Promise::any() at previous call
        .then(asyn!(_, (url, result) => {
            match result {
                Ok(response) => info!("{url} was fastest with status {}", response.status),
                Err(error) => info!("{url} failed first with {error}")
            }
            Promise::pass()
        }))
        // `pecs` comes with iterator extension for promises.
        // It allows you to make the same calls, but sligtly simpler:
        .then(asyn!(_, _ => {
            info!("Requesting all urls using iterator extension");

            // Instead of creating Vec<Promise> manually and pass it
            // to Promise:all(), you can call .promise().all() on
            // any Iterator<Item =Promise>
            URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send().with(url))
                .promise()
                .all()
        }))
        .then(asyn!(_, resolved => {
            info!("Responses:");
            for (url, result) in resolved.iter() {
                match result {
                    Ok(response) => info!("  [{}] {url}", response.status),
                    Err(error) => info!("  Can't get {url}: {error}")
                }
            }
            Promise::pass()
        }))
        // and the same way .prmise().any() could be done:
        .then(asyn!(_, _ => {
            info!("Requesting any of urls using iterator extension");
            URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send().with(url))
                .promise()
                .any()
        }))
        .then(asyn!(_, (url, result) => {
            match result {
                Ok(response) => info!("{url} was fastest with status {}", response.status),
                Err(error) => info!("{url} failed first with {error}")
            }
            Promise::pass()
        }))
        // In previouse calls state (the first argument to asyn!) was ignored,
        // () was passed between calls.
        // Let's try to report how long it is takes to get all requests and
        // the fastest one.
        // To do it, you can store current time and calculate the difference in
        // the next call.
        // To get the time you can use bevy Time resource
        .then(asyn!(state, _, time: Res<Time> => {
            info!("Tracking time to get response from all requests");
            let started_at = time.elapsed_seconds();
            let requests = URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send())
                .collect::<Vec<_>>();
            state
                // change the state value
                .with(started_at)
                // .all() method of the PromiseState takes same
                // arguments as PromiseAll and does the same job
                // but also bypys the state to the next calls
                .all(requests)
        }))
        // resolved value doesn't important here
        // state is more interesting: its type is
        // PromiseState<f32>
        .then(asyn!(state, _, time: Res<Time> => {
            let current_time = time.elapsed_seconds();
            let delta = current_time - state.value;
            info!("Time to complete all requests: {delta:0.3}");

            // make new call right here.
            info!("Tracking time to get response from the fastest request");
            // store current time to make proper calculations after resolve
            state.value = current_time;
            let requests = URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send())
                .collect::<Vec<_>>();
            state.any(requests)
        }))
        .then(asyn!(state, _, time: Res<Time> => {
            let current_time = time.elapsed_seconds();
            let delta = current_time - state.value;
            info!("Time to complete fastest request: {delta:0.3}");
            state.value = current_time;

            // If you want to use iterator extension method
            // you need to pass context manually.
            info!("Tracking one more time the fastest one");
            URLS
                .into_iter()
                .map(|url| asyn::http::get(url).send())
                .promise()
                .any()
                .with(state.value)
        }))
        .then(asyn!(state, _, time: Res<Time> => {
            let current_time = time.elapsed_seconds();
            let delta = current_time - state.value;
            info!("Time to complete fastest request: {delta:0.3}");
            state.pass()
        }))
        // close app at the end
        .then(asyn!(_, _ => {
            info!("See you!");
            asyn::app::exit()
        })),
    );
}
