//! ## About
//! `pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you to execute code asynchronously
//! by chaining multiple promises as part of Bevy's `ecs` environment.
//!
//! `pecs` stands for `Promise Entity Component System`.
//!
//! ### Features
//!
//! - Promise chaining with [`then()`][core::PromiseLikeBase::then]/
//!   [`then_repeat()`][core::PromiseLike::then_repeat]
//! - State passing (`state` for promises is like `self` for items).
//! - Complete type inference (the next promise knows the type of the previous result).
//! - Out-of-the-box timer, UI and HTTP promises via stateless [`asyn`][mod@prelude::asyn] mod and
//!   stateful  [`state.asyn()`][core::PromiseState::asyn] method.
//! - Custom promise registration (add any asynchronous function you want!).
//! - [System parameters](https://docs.rs/bevy/latest/bevy/ecs/system/trait.SystemParam.html) fetching
//!   (promise `asyn!` functions accept the same parameters as Bevy systems do).
//! - Nested promises (with chaining, obviously).
//! - Combining promises with `any/all` for tuple/vec of promises via stateless [`any()`][core::Promise::any]
//!   /[`all()`][core::Promise::all()] methods or stateful
//!   [`state.any()`][core::PromiseState::any]/[`state.all()`][core::PromiseState::all] methods.
//! - State mapping via [`with(value)`][core::PromiseLikeBase::with]/
//!   [`map(func)`][core::PromiseLikeBase::map]
//!   (changes state type over chain calls).
//! - Result mapping via [`with_result(value)`][core::PromiseLikeBase::with_result]/
//!   [`map_result(func)`][core::PromiseLikeBase::map_result] (changes result type over
//!   chain calls).
//!
//! ## Example
//! ```rust
//! use bevy::prelude::*;
//! use pecs::prelude::*;
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugin(PecsPlugin)
//!         .add_startup_system(setup)
//!         .run();
//! }
//!
//! fn setup(mut commands: Commands, time: Res<Time>) {
//!     let start = time.elapsed_seconds();
//!     commands
//!         // create PromiseLike chainable commands
//!         // with the current time as state
//!         .promise(|| start)
//!
//!         // will be executed right after current stage
//!         .then(asyn!(state => {
//!             info!("Wait a second..");
//!             state.asyn().timeout(1.0)
//!         }))
//!
//!         // will be executed after in a second after previous call
//!         .then(asyn!(state => {
//!             info!("How large is is the Bevy main web page?");
//!             state.asyn().http().get("https://bevyengine.org")
//!         }))
//!
//!         // will be executed after request completes
//!         .then(asyn!(state, result => {
//!             match result {
//!                 Ok(response) => info!("It is {} bytes!", response.bytes.len()),
//!                 Err(err) => info!("Ahhh... something goes wrong: {err}")
//!             }
//!             state.pass()
//!         }))
//!
//!         // will be executed right after the previous one
//!         .then(asyn!(state, time: Res<Time> => {
//!             let duration = time.elapsed_seconds() - state.value;
//!             info!("It tooks {duration:0.2}s to do this job.");
//!             info!("Exiting now");
//!             asyn::app::exit()
//!         }));
//! }
//! ```
//!
//! There is otput of the above example, pay some attention to time stamps:
//!
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
//! To create a new promise, you can use one of the following methods:
//!
//! - [`Promise::start()`][prelude::Promise::start]: Creates a new
//!   promise without initial state.
//! - [`Promise::new(state)`][prelude::Promise::new]: Creates a new promise
//!   with the specified initial state.
//! - [`Promise::register(on_invoke, on_discard)`][prelude::Promise::register]:
//!   Registers a new promise with the specified `on_invoke` and `on_discard` callbacks.
//!
//! It is also possible to create [`PromiseLike`][core::PromiseLike] promise containers
//! that act just like promises with:
//! - [`commands.promise(|| state)`][core::PromiseCommandsExtension::promise] for creating
//!   `PromiseLike` from default state
//! - [`commands.promise(promise)`][core::PromiseCommandsExtension::promise] from existing
//!   promise
//!
//! The resulting promise has the signature [`Promise<S, R>`][prelude::Promise], where `R`
//! is the type of the result and `S` is the type of the promise state. Note that `R` and `S`
//! must be `'static` types, so references or lifetime types are not allowed.
//!
//! Promises can be chained together using the [`then()`][core::PromiseLikeBase::then] method, which
//! takes an [`Asyn`][struct@core::Asyn] function created with the [`asyn!`][prelude::asyn!] macro. The
//! [`Asyn`][struct@core::Asyn] function takes the promise state as its first argument, and the promise
//! result as its second argument. Any additional arguments are optional and correspond to the
//! [system parameters][bevy::ecs::system::SystemParam] used in Bevy's ECS. This allows you to do
//! inside an [`Asyn`][struct@core::Asyn] function everything you can do inside a regular system, while
//! still keeping track of system parameter states.
//!
//! If the result of the [`Asyn`][struct@core::Asyn] function is
//! [`Resolve`][core::PromiseResult::Resolve], the result is passed immediately to
//! the next promise in the chain. If the result is [`Await`][core::PromiseResult::Await],
//! the next promise in the chain is resolved when the nested promise is resolved. The type of the next
//! promise's state and result arguments are inferred from the result of the previous promise:
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
//! ## State
//! When working with asynchronous operations, it is often useful to carry a state along with
//! the promises in a chain. The `pecs` provides a convenient way to do this using the
//! [`PromiseState<S>`][core::PromiseState] type.
//!
//! [`PromiseState`][core::PromiseState] is a wrapper around a `'static S` value. This value
//! can be accessed and modified using `state.value`. [`PromiseState`][core::PromiseState] also
//! implements [`Deref`][`std::ops::Deref`], so in most you cases you can omit `.value`.
//!
//! To use [`PromiseState`][core::PromiseState], you don't create it directly. Instead, it is automatically passed
//! as the first argument to the [`Asyn`][struct@core::Asyn] function.
//!
//! For example, suppose you have a stateful promise that increments a counter, waits for some
//! time, and then logs the counter value. Here's how you could implement it:
//! ```rust
//! fn setup(mut commands: Commands) {
//!     commands
//!         // create a promise with int state
//!         .promise(|| 0)
//!         .then(asyn!(state => {
//!             state.value += 1;
//!             state.asyn().timeout(1.0)
//!         }))
//!         .then(asyn!(state => {
//!             info!("Counter value: {}", state.value);
//!         }));
//! }
//! ```
//! In this example, we start with an initial state value of 0 and increment it by 1 in the first
//! promise. We then use `state.asyn().timeout()` to wait for one second before logging the final
//! state value. The asyn method returns an [`AsynOps<S>`][core::AsynOps] value, which can be used
//! to create new promises that are associated with the current state.
//!
//! [`PromiseState`][core::PromiseState] can be used with other pecs constructs like
//! [`then()`][core::PromiseLikeBase::then], [`repeat()`][core::Promise::repeat()] or
//! [`all()`][core::Promise::all] to create complex promise chains that carry stateful values.
//! Here's an example that uses [`any`][core::PromiseState::all] method to create a promise that
//! resolves when any of provided promises have resolved with current state itself:
//!
//! ```rust
//! fn setup(mut commands: Commands, time: Res<Time>) {
//!     let start = time.elapsed_seconds();
//!     commands
//!         // use `start: f32` as a state
//!         .promise(|| start)
//!         // state is f32 here
//!         .then(asyn!(state => {
//!             state.any((
//!                 asyn::timeout(0.4),
//!                 asyn::http::get("https://bevyengine.org").send()
//!             ))
//!         }))
//!         // state is f32 as well
//!         .then(asyn!(state, (timeout, response) => {
//!             if timeout.is_some() {
//!                 info!("Bevy site is not fast enoutgh");
//!             } else {
//!                 let status = if let Ok(response) = response.unwrap() {
//!                     response.status.to_string()
//!                 } else {
//!                     format!("Error")
//!                 };
//!                 info!("Bevy respond pretty fast with {status}");
//!             }
//!             // pass the state to the next call
//!             state.pass()
//!         }))
//!         // it is still f32
//!         .then(asyn!(state, time: Res<Time> {
//!             let time_to_process = time.elapsed_seconds() - state.value;
//!             info!("Done in {time_to_process:0.2}s");
//!         }));
//! }
//! ```
//!
//! See [combine_vecs](https://github.com/jkb0o/pecs/blob/master/examples/combine_vecs.rs)
//! and [confirmation](https://github.com/jkb0o/pecs/blob/master/examples/confirmation.rs)
//! examples to better understand the `state` behaviour.
//!
//! ## Work in Progress
//! This crate is pretty young. API could and will change. App may crash. Some
//! promises could silently drop. Documentation is incomplete.
//!
//! But. But. Examples works like a charm. And this fact gives us a lot of hope.
//!
//! There are a lot docs planned to put here, but I believe it is better to release `something`
//! then `perfect`.

/// All you need is `use pecs::prelude::*`
pub mod prelude {
    // structs
    #[doc(inline)]
    pub use pecs_core::Promise;
    #[doc(inline)]
    pub use pecs_core::PromiseCommand;
    #[doc(inline)]
    pub use pecs_core::PromiseId;
    #[doc(inline)]
    pub use pecs_core::Repeat;

    // traits
    #[doc(inline)]
    pub use pecs_core::timer::TimerOpsExtension;
    #[doc(inline)]
    pub use pecs_core::ui::UiOpsExtension;
    #[doc(inline)]
    pub use pecs_core::PromiseCommandsExtension;
    #[doc(inline)]
    pub use pecs_core::PromiseLike;
    #[doc(inline)]
    pub use pecs_core::PromiseLikeBase;
    #[doc(inline)]
    pub use pecs_core::PromisesExtension;
    #[doc(inline)]
    pub use pecs_http::HttpOpsExtension;

    // macros
    #[doc(inline)]
    pub use pecs_core::Asyn;
    #[doc(inline)]
    pub use pecs_macro::asyn;

    use bevy::prelude::*;
    pub struct PecsPlugin;
    impl Plugin for PecsPlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<pecs_core::timer::Timers>();
            app.add_system(pecs_core::timer::process_timers);

            app.add_plugin(pecs_http::PromiseHttpPlugin);
            app.add_plugin(pecs_core::ui::PromiseUiPlugin);
        }
    }

    /// Out-of-the box async operations
    pub mod asyn {
        #[doc(inline)]
        pub use pecs_core::app;
        #[doc(inline)]
        pub use pecs_core::timer::timeout;
        #[doc(inline)]
        pub use pecs_core::ui::asyn as ui;
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
