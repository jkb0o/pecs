//! ## About
//! `pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you
//! to execute code asynchronously by chaining multple promises with respect
//! of Bevy's `ecs` enviroment.
//!
//! ### Features
//! - promise chaining with ([`then()`][prelude::Promise::then],
//!   [`ok_then()`][prelude::Promise::ok_then] or [`or_else()`][core::IncompletePromise::or_else])
//! - state passing (`state` for promises is like `self` for items)
//! - complete type inference (the next promise knows tye types of previous result)
//! - out-of-the-box timer and http promises via [`asyn`][mod@prelude::asyn] mod and stateful
//!   [`state.asyn()`][core::PromiseState::asyn]
//! - custom promise registretion (add any asyn function you want!)
//! - [system params][bevy::ecs::system::SystemParam] fetching
//!   (promise `asyn!` funcs acccepts the same params the bevy systems does)
//! - nested promises (with chaining, obviously)
//! - combining promises with any/all for tuple/vec of promises via stateles
//!   [`Promise::any()`][core::PromiseAnyMethod::any()]/
//!   [`Promise::all()`][core::PromiseAllMethod::all()]
//!   or stateful [`state.any()`][core::PromiseState::any]/
//!   [`state.all()`][core::PromiseState::all]
//! - state mapping via [`with(value)`][prelude::Promise::with]/
//!   [`map(func)`][prelude::Promise::map] (change state type over chain calls)
//! - result mapping via [`with_ok(value)`][prelude::Promise::with_ok]/
//!   [`map_ok(func)`][prelude::Promise::map_ok] (change Ok type over chain calls)
//! - error mapping via [`with_err(value)`][prelude::Promise::with_err]/
//!   [`map_err(func)`][prelude::Promise::map_err] (change Err type over chain calls)
//!
//! ## Example
//! ```rust
//! use bevy::{prelude::*, app::AppExit};
//! use pecs::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugin(PromisePlugin)
//!         .add_startup_system(setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands) {
//!     commands.add(
//!         Promise::start(asyn!(state, time: Res<Time> => {
//!             info!("Wait a second..");
//!             let started_at = time.elapsed_seconds();
//!             state.with(started_at).asyn().timeout(1.0)
//!         }))
//!         .then(asyn!(state, _ => {
//!             info!("Looks like I need to know how large is the Bevy main web page!");
//!             state.asyn().http().get("https://bevyengine.org").send()
//!         }))
//!         .then(asyn!(state, result => {
//!             match result {
//!                 Ok(response) => info!("It is {} bytes!", response.bytes.len()),
//!                 Err(err) => info!("Ahhh... something goes wrong: {err}")
//!             }
//!             state.done()
//!         }))
//!         .then(asyn!(state, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
//!             let duration = time.elapsed_seconds() - state.value;
//!             info!("It tooks {duration:0.2}s to do this job.");
//!             info!("Exiting now");
//!             exit.send(AppExit);
//!             state.done()
//!         }))
//!     );
//! }
//! ```
//! There is otput of the above example, pay some attention to time stamps:
//! ```text
//! 15:52:20.459635Z  INFO bevy_render::renderer: AdapterInfo { ... }
//! 15:52:20.643082Z  INFO simple: Wait a second..
//! 15:52:21.659898Z  INFO simple: Looks like I need to know how large is the Bevy main web page!
//! 15:52:21.775228Z  INFO simple: It is 17759 bytes!
//! 15:52:21.775319Z  INFO simple: It tooks 1.13s to do this job.
//! 15:52:21.775342Z  INFO simple: Exiting now
//! ```
//!
//! ## Basics
//!
//! You create new promises via [`Promise::start()`][prelude::Promise::start],
//! [`Promise::new(state)`][prelude::Promise::new] or
//! [`Promise::register(on_invoke, on_discard)`][prelude::Promise::register]. This
//! gives you a promise with signature [`Promise<R, E, S>`][prelude::Promise] where
//! `R` is the type of successful result, `E` is the type of error and `S` is the
//! type of the promise state. The only limitations of `R`, `E` and `S`: it should be
//! `'static`. So no references or liftime types, sorry.
//!
//! You chain promises ([`then()`][prelude::Promise::then],
//! [`ok_then()`][prelude::Promise::ok_then] or [`or_else()`][core::IncompletePromise::or_else])
//! by passing [`AsynFunction`][core::AsynFunction] created with [`asyn!`][prelude::asyn!]
//! macro. This function takes a state as first argument, and result as second argument.
//! Other optional arguments are [system params][bevy::ecs::system::SystemParam], so you can
//! do inside [`AsynFunction`][core::AsynFunction] everything you do inside your systems. The
//! only difference is, this functions executed only once and do not track state changes,
//! so any change filters are useless.
//!
//! The result of [`AsynFunction`][core::AsynFunction] passes to the next promise in the chain
//! immidiatly if it is the [`PromiseResult::Resolve`][core::PromiseResult::Resolve]/
//! [`PromiseResult::Reject`][core::PromiseResult::Reject], or when nested promise got resolved
//! if it is [`PromiseResult::Await`][core::PromiseResult::Await]. The type of the next promise
//! state/result arguments are infered from the result of previous promise:
//! ```rust
//! fn inference(mut commands: Commands) {
//!     commands.add(
//!         Promise::start(asyn!(_ => {
//!             Promise::ok("Hello!")
//!         }))
//!         
//!         // _: PromiseState<()>, result: Result<'static str, ()>
//!         .then(asyn!(_, result => {
//!             info!("#1 resolved with {}", result.unwrap());
//!             Promise::ok("Hello?")
//!         }))
//!
//!         // ok_then used to take successfull results
//!         // _: PromiseState<()>, result: 'static str
//!         .ok_then(asyn!(_, result => {
//!             info!("#2 resolved with {result}");
//!             Promise::ok(result.to_string())
//!         }))
//!
//!         // ok_then used to take successfull results
//!         // _: PromiseState<()>, result: String
//!         .ok_then(asyn!(_, result => {
//!             info!("#3 resolved with {result}");
//!             Promise::ok(result)
//!         }))
//!
//!         // asyn::timeout(d) returns Promise<(), (), ()>
//!         // that resolves after after `d` seconds
//!         .ok_then(asyn!(_, result => {
//!             info!("#4 resolved with {result}");
//!             asyn::timeout(1.)
//!         }))
//!         // _: PromiseState<()>, result: ()
//!         .ok_then(asyn!(_, result => {
//!             info!("#5 resolved with {result:?}");
//!             Promise::ok(())
//!         }))
//!     );
//! }
//! ```
//!
//! ## Work in Progress
//! This crate is more like an experimental-proof-of-concept then a production-ready library.
//!  API could and will change. App will crash (there are some untested unsafe blocks), some
//! promises will silently drop (there are stil no unit tests), documentation is incomplete
//! and so on. But. But. Examples works like a charm. And this fact gives us a lot of hope.
//!
//! There are a lot docs planned to put here, but I believe it is better to release something
//! then perfect. So I just put complex example here (with all features covered) and wish you a
//! good luck.

/// All you need is `use pecs::prelud::*`
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
    pub use pecs_core::PromiseAllMethod;
    #[doc(inline)]
    pub use pecs_core::PromiseAnyMethod;
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
