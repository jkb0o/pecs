//! ## About
//! `pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you
//! to execute code asynchronously by chaining multple promises as part of Bevy's
//! `ecs` enviroment.
//! 
//! `pecs`stands for `Promise Entity Component System`
//!
//! ### Features
//! - promise chaining with [`then()`][prelude::Promise::then]
//! - state passing (`state` for promises is like `self` for items)
//! - complete type inference (the next promise knows the type of the previous result)
//! - out-of-the-box timer and http promises via stateless[`asyn`][mod@prelude::asyn]
//!   mod and stateful [`state.asyn()`][core::PromiseState::asyn]
//! - custom promise registretion (add any asyn function you want!)
//! - [system params](https://docs.rs/bevy/latest/bevy/ecs/system/trait.SystemParam.html) fetching
//!   (promise `asyn!` funcs accepts the same params the bevy systems does)
//! - nested promises (with chaining, obviously)
//! - combining promises with any/all for tuple/vec of promises via stateles
//!   [`Promise::any()`][core::PromiseAnyMethod::any()]/
//!   [`Promise::all()`][core::PromiseAllMethod::all()]
//!   or stateful [`state.any()`][core::PromiseState::any]/
//!   [`state.all()`][core::PromiseState::all]
//! - state mapping via [`with(value)`][prelude::Promise::with]/
//!   [`map(func)`][prelude::Promise::map] (change state type over chain calls)
//! - result mapping via [`with_result(value)`][prelude::Promise::with_result]/
//!   [`map_result(func)`][prelude::Promise::map_result] (change result type over chain calls)
//!
//! ## Example
//! ```rust
//!  use bevy::{app::AppExit, prelude::*};
//!  use pecs::prelude::*;
//!  fn main() {
//!      App::new()
//!          .add_plugins(DefaultPlugins)
//!          .add_plugin(PecsPlugin)
//!          .add_startup_system(setup)
//!          .run();
//!  }
//!  
//!  fn setup(mut commands: Commands) {
//!      commands.add(
//!          Promise::start(asyn!(state, time: Res<Time> => {
//!              info!("Wait a second..");
//!              let started_at = time.elapsed_seconds();
//!              state.with(started_at).asyn().timeout(1.0)
//!          }))
//!          .then(asyn!(state, _ => {
//!              info!("How large is is the Bevy main web page?");
//!              state.asyn().http().get("https://bevyengine.org")
//!          }))
//!          .then(asyn!(state, result => {
//!              match result {
//!                  Ok(response) => info!("It is {} bytes!", response.bytes.len()),
//!                  Err(err) => info!("Ahhh... something goes wrong: {err}")
//!              }
//!              state.pass()
//!          }))
//!          .then(asyn!(state, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
//!              let duration = time.elapsed_seconds() - state.value;
//!              info!("It tooks {duration:0.2}s to do this job.");
//!              info!("Exiting now");
//!              exit.send(AppExit);
//!              state.pass()
//!          })),
//!      );
//!  }

//! ```
//! There is otput of the above example, pay some attention to time stamps:
//! ```text
//! 18.667 INFO bevy_render::renderer: AdapterInfo { ... }
//! 18.835 INFO simple: Wait a second..
//! 19.842 INFO simple: How large is is the Bevy main web page?
//! 19.924 INFO simple: It is 17759 bytes!
//! 19.924 INFO simple: It tooks 1.09s to do this job.
//! 19.924 INFO simple: Exiting now
//! ```
//!
//! ## Basics
//!
//! You create new promises via [`Promise::start()`][prelude::Promise::start],
//! [`Promise::new(state)`][prelude::Promise::new] or
//! [`Promise::register(on_invoke, on_discard)`][prelude::Promise::register]. This
//! gives you a promise with signature [`Promise<S, R>`][prelude::Promise] where
//! `R` is the type of result and `S` is the type of the promise state. The only
//! limitations of `R` and `S`: it should be `'static`. So no references or liftime
//! types, sorry.
//!
//! You chain promises with [`then()`][prelude::Promise::then], by passing
//! [`AsynFunction`][core::AsynFunction] created with [`asyn!`][prelude::asyn!]
//! macro. This function takes a state as first argument, and result as second argument.
//! Other optional arguments are [system params][bevy::ecs::system::SystemParam], so you can
//! do inside [`AsynFunction`][core::AsynFunction] everything you do inside your systems. The
//! only difference is, this functions executed only once and do not track state changes,
//! so any change filters are useless.
//!
//! The result of [`AsynFunction`][core::AsynFunction] passes to the next promise in the chain
//! immidiatly if it is the [`PromiseResult::Resolve`][core::PromiseResult::Resolve],
//! or when nested promise got resolved if it is [`PromiseResult::Await`][core::PromiseResult::Await].
//! The type of the next promise state/result arguments are infered from the result of previous promise:
//! ```rust
//! use bevy::prelude::*;
//! use pecs::prelude::*;
//! fn inference(mut commands: Commands) {
//!     commands.add(
//!         Promise::start(asyn!(_ => {
//!             Promise::resolve("Hello!")
//!         }))
//!         
//!         // _: PromiseState<()>, result: &str
//!         .then(asyn!(_, result => {
//!             info!("#1 resolved with {}", result);
//!             Promise::resolve("Hello?")
//!         }))
//!         // _: PromiseState<()>, result: &str
//!         .then(asyn!(_, result => {
//!             info!("#2 resolved with {result}");
//!             Promise::resolve(result.to_string())
//!         }))
//!         // ok_then used to take successfull results
//!         // _: PromiseState<()>, result: String
//!         .then(asyn!(_, result => {
//!             info!("#3 resolved with {result}");
//!             Promise::resolve(result)
//!         }))
//!         // asyn::timeout(d) returns Promise<(), (), ()>
//!         // that resolves after after `d` seconds
//!         .then(asyn!(_, result => {
//!             info!("#4 resolved with {result}");
//!             asyn::timeout(1.)
//!         }))
//!         // _: PromiseState<()>, result: ()
//!         .then(asyn!(_, result => {
//!             info!("#5 resolved with {result:?}");
//!             Promise::resolve(())
//!         }))
//!     );
//! }
//! ```
//!
//! ## Work in Progress
//! This crate is more like an experimental-proof-of-concept than a production-ready library.
//! API could and will change. App will crash (there are some untested unsafe blocks), some
//! promises will silently drop (there are stil no unit tests), documentation is incomplete
//! and so on. But. But. Examples works like a charm. And this fact gives us a lot of hope.
//!
//! There are a lot docs planned to put here, but I believe it is better to release something
//! then perfect. So I just put complex example here (with all features covered) and wish you a
//! good luck.
//! 
//! ## Complex Example
//! ```rust
//! use bevy::app::AppExit;
//! use bevy::prelude::*;
//! use pecs::prelude::*;
//! 
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugin(PecsPlugin)
//!         .add_startup_system(setup)
//!         .run();
//! }
//! 
//! fn setup(mut commands: Commands) {
//!     commands.spawn(Camera2dBundle::default());
//!     commands.add(
//!         Promise::start(asyn!(s, time: Res<Time> => {
//!             let t = time.elapsed_seconds();
//!             info!("start with 31, started at {t}, start time stored in state.");
//!             s.map(|_| t).resolve(31)
//!         }))
//!         .then(asyn!(s, r => {
//!             info!("Continue first time with result: {r}, incrementing");
//!             s.resolve(r + 1)
//!         }))
//!         .then(asyn!(s, r => {
//!             info!("Continue second time with result: {r}");
//!             s.resolve(r)
//!         }))
//!         .then(asyn!(s, r => {
//!             info!("continue third time with result: {r}");
//!             s.asyn().timeout(1.5).with_result(r + 1)
//!         }))
//!         .then(asyn!(s, r => {
//!             info!("continue after 1.5 sec delay with {r}");
//!             s.asyn().timeout(1.5)
//!         }))
//!         .then(asyn!(s, _, mut commands: Commands => {
//!             info!("complete after 1.5 sec delay, adding custom command");
//!             commands.add(|_: &mut World| info!("Executing custom command at the end."));
//!             let timeout = rand();
//!             info!("Requesting https:://google.com with timeout {timeout:0.2}s");
//!             s.any((
//!                 // wait for first completed promise
//!                 asyn::timeout(timeout),
//!                 asyn::http::get("https://google.com").send(),
//!             ))
//!         }))
//!         .then(asyn!(s, (timeout, response) => {
//!             if timeout.is_some() {
//!                 info!("Request timed out");
//!             } else {
//!                 match response.unwrap() {
//!                     Ok(r) => info!("Respond faster then timeout with {}", r.status),
//!                     Err(e) => info!("Respond faster then timeout with error: {e}"),
//!                 }
//!             }
//!             s.pass()
//!         }))
//!         .then(asyn!(s, _ => {
//!             s.all((
//!                 asyn::http::get("https://google.com").send(),
//!                 asyn::http::get("https://bevyengine.org").send(),
//!             ))
//!         }))
//!         .then(asyn!(s, r => {
//!             let (google, bevy) = r;
//!             if let Ok(google) = google {
//!                 info!("Google respond with {}", google.status);
//!             } else {
//!                 info!("Google respond error");
//!             }
//!             if let Ok(bevy) = bevy {
//!                 info!("Bevy respond with {}", bevy.status);
//!             } else {
//!                 info!("Bevy respond error");
//!             }
//!             s.pass()
//!         }))
//!         .then(asyn!(s, _ => {
//!             info!("Requesting any");
//!             ["https://google.com", "https://bevyengine.org", "https://github.com"]
//!                 .iter()
//!                 .map(|url| {
//!                     info!("  {url}");
//!                     asyn::http::get(url).send().with(*url)
//!                 })
//!                 .promise()
//!                 .any()
//!                 .with(s.value)
//!         }))
//!         .then(asyn!(s, (url, result) => {
//!             let resp = match result {
//!                 Ok(r) => format!("{}", r.status),
//!                 Err(e) => e,
//!             };
//!             info!("{url} respond faster then others with {resp}");
//!             s.pass()
//!         }))
//!         .then(asyn!(s, _ => {
//!             info!("Requesting all");
//!             ["https://google.com", "https://bevyengine.org", "https://github.com"]
//!                 .iter()
//!                 .map(|url| {
//!                     info!("  {url}");
//!                     url
//!                 })
//!                 .map(|url| asyn::http::get(url).send().with(*url))
//!                 .promise()
//!                 .all()
//!                 .with(s.value)
//!         }))
//!         .then(asyn!(s, r => {
//!             info!("Services responded:");
//!             for (url, r) in r.iter() {
//!                 match r {
//!                     Ok(r) => info!("  {url}: {}", r.status),
//!                     Err(e) => info!("  {url}: {e}"),
//!                 }
//!             }
//!             s.pass()
//!         }))
//!         .then(asyn!(s, _ => {
//!             info!("requesing https://bevyengine.org");
//!             s.asyn().http().get("https://bevyengine.org")
//!         }))
//!         .then(asyn!(s, r => {
//!             match r {
//!                 Ok(r) => info!("Bevy respond with {}, body size: {}", r.status, r.bytes.len()),
//!                 Err(e) => warn!("Error requesting Bevy: {e}"),
//!             }
//!             s.then(log_request("https://google.com")).then(asyn!(|s, r| {
//!                 info!("Request done in {r} secs");
//!                 s.pass()
//!             }))
//!         }))
//!         .then(asyn!(s, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
//!             info!(
//!                 "Done, time to process: {} (start time took from state {}",
//!                 time.elapsed_seconds() - s.value,
//!                 s
//!             );
//!             exit.send(AppExit);
//!             s.pass()
//!         })),
//!     );
//! }
//! 
//! /// Returns a promise that requests `url`, logs the process
//! /// and resolves with seconds spent to complete requests as `f32`
//! fn log_request(url: &'static str) -> Promise<(), f32> {
//!     Promise::new(
//!         url,
//!         asyn!(|s, time: Res<Time>| {
//!             let url = s.value;
//!             let start = time.elapsed_seconds();
//!             info!("Requesting {url} at {start:0.2}");
//!             s.map(|url| (url, start)).asyn().http().get(url)
//!         }),
//!     )
//!     .then(asyn!(|s, r, time: Res<Time>| {
//!         match r {
//!             Ok(r) => info!("{} respond with {}, body size: {}", s.value.0, r.status, r.bytes.len()),
//!             Err(e) => warn!("Error requesting {}: {e}", s.value.0),
//!         }
//!         let duration = time.elapsed_seconds() - s.value.1;
//!         s.map(|_| ()).resolve(duration)
//!     }))
//! }
//! 
//! // almost implemeted by chatgpt
//! pub fn rand() -> f32 {
//!     use std::hash::{Hash, Hasher};
//!     let epoch = std::time::SystemTime::now()
//!         .duration_since(std::time::UNIX_EPOCH)
//!         .unwrap()
//!         .as_nanos();
//!     let pid = std::process::id();
//!     let mut hasher = std::collections::hash_map::DefaultHasher::new();
//!     (epoch, pid).hash(&mut hasher);
//!     let seed = hasher.finish() as u64;
//!     (seed as f32) / u64::MAX as f32
//! }
//! ```
//! 
//! Output:
//! ```text
//! 54.424  INFO complex: start with 31, started at 0.2795026, start time stored in state.
//! 54.424  INFO complex: Continue first time with result: 31, incrementing
//! 54.424  INFO complex: Continue second time with result: 32
//! 54.424  INFO complex: continue third time with result: 32
//! 55.940  INFO complex: continue after 1.5 sec delay with 33
//! 57.423  INFO complex: complete after 1.5 sec delay, adding custom command
//! 57.423  INFO complex: Requesting https:://google.com with timeout 0.23s
//! 57.424  INFO complex: Executing custom command at the end.
//! 57.639  INFO complex: Request timed out
//! 58.256  INFO complex: Google respond with 200
//! 58.256  INFO complex: Bevy respond with 200
//! 58.257  INFO complex: Requesting any
//! 58.257  INFO complex:   https://google.com
//! 58.257  INFO complex:   https://bevyengine.org
//! 58.257  INFO complex:   https://github.com
//! 58.322  INFO complex: https://bevyengine.org respond faster then others with 200
//! 58.322  INFO complex: Requesting all
//! 58.322  INFO complex:   https://google.com
//! 58.322  INFO complex:   https://bevyengine.org
//! 58.322  INFO complex:   https://github.com
//! 59.123  INFO complex: Services responded:
//! 59.123  INFO complex:   https://google.com: 200
//! 59.123  INFO complex:   https://bevyengine.org: 200
//! 59.123  INFO complex:   https://github.com: 200
//! 59.123  INFO complex: requesing https://bevyengine.org
//! 59.205  INFO complex: Bevy respond with 200, body size: 17759
//! 59.205  INFO complex: Requesting https://google.com at 5.06
//! 59.806  INFO complex: https://google.com respond with 200, body size: 18275
//! 59.806  INFO complex: Request done in 0.6004901 secs
//! 59.806  INFO complex: Done, time to process: 5.3819447 (start time took from state PromiseState(0.2795026)
//! ```


/// All you need is `use pecs::prelude::*`
pub mod prelude {
    #[doc(inline)]
    pub use pecs_core::Promise;
    #[doc(inline)]
    pub use pecs_core::PromiseCommand;
    #[doc(inline)]
    pub use pecs_core::PromiseId;

    #[doc(inline)]
    pub use pecs_core::timer::TimerOpsExtension;
    #[doc(inline)]
    pub use pecs_core::PromisesExtension;
    #[doc(inline)]
    pub use pecs_core::PromiseCommandsExtension;
    #[doc(inline)]
    pub use pecs_http::HttpOpsExtension;
    #[doc(inline)]
    pub use pecs_macro::asyn;

    use bevy::prelude::*;
    pub struct PecsPlugin;
    impl Plugin for PecsPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<pecs_core::timer::Timers>();
            app.add_system(pecs_core::timer::process_timers);

            app.add_plugin(pecs_http::PromiseHttpPlugin);
        }
    }

    /// Out-of-the box async operations
    pub mod asyn {
        #[doc(inline)]
        pub use pecs_core::timer::timeout;
        #[doc(inline)]
        pub use pecs_http::asyn as http;
    }
}

#[doc(inline)]
pub use pecs_core as core;
#[doc(inline)]
pub use pecs_core::timer;
#[doc(inline)]
pub use pecs_http as http;
