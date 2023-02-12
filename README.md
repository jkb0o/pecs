## About
`pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you
to execute code asynchronously by chaining multple promises as part of Bevy's `ecs` enviroment.

`pecs`stands for `Promise Entity Component System`.

Resources:
- [Docs](https://docs.rs/pecs/)
- [Examples](https://github.com/jkb0o/pecs/tree/master/examples)
- [Report an issue](https://github.com/jkb0o/pecs/issues/new)
- [Provide an idea](https://github.com/jkb0o/pecs/issues/new)

### Features
- promise chaining with `then()`
- state passing (`state` for promises is like `self` for items)
- complete type inference (the next promise knows the type of the previous result)
- out-of-the-box timer and http promises via `asyn` mod and stateful `state.asyn()`
- custom promise registretion (add any asyn function you want!)
- `system params` fetching (promise `asyn!` funcs accepts the same params
  the bevy systems does)
- nested promises (with chaining, obviously)
- combining promises with any/all for tuple/vec of promises via stateles
  `Promise::any()`/`Promise::all()` or stateful `state.any()`/`state.all()`
- state mapping via `with(value)`/`map(func)` (change state type/value over chain calls)
- result mapping via `with_ok(value)`/`map_ok(func)` (change result type/value over chain calls)

## Example
```rust
 use bevy::{app::AppExit, prelude::*};
 use pecs::prelude::*;
 fn main() {
     App::new()
         .add_plugins(DefaultPlugins)
         .add_plugin(PecsPlugin)
         .add_startup_system(setup)
         .run();
 }
 
 fn setup(mut commands: Commands) {
     commands.add(
         Promise::start(asyn!(state, time: Res<Time> => {
             info!("Wait a second..");
             let started_at = time.elapsed_seconds();
             state.with(started_at).asyn().timeout(1.0)
         }))
         .then(asyn!(state, _ => {
             info!("How large is is the Bevy main web page?");
             state.asyn().http().get("https://bevyengine.org")
         }))
         .then(asyn!(state, result => {
             match result {
                 Ok(response) => info!("It is {} bytes!", response.bytes.len()),
                 Err(err) => info!("Ahhh... something goes wrong: {err}")
             }
             state.pass()
         }))
         .then(asyn!(state, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
             let duration = time.elapsed_seconds() - state.value;
             info!("It tooks {duration:0.2}s to do this job.");
             info!("Exiting now");
             exit.send(AppExit);
             state.pass()
         })),
     );
 }
```
There is otput of the above example, pay some attention to time stamps:
```text
18.667 INFO bevy_render::renderer: AdapterInfo { ... }
18.835 INFO simple: Wait a second..
19.842 INFO simple: How large is is the Bevy main web page?
19.924 INFO simple: It is 17759 bytes!
19.924 INFO simple: It tooks 1.09s to do this job.
19.924 INFO simple: Exiting now
```

## Work in Progress

This repo is more like an experimental-proof-of-concept than a production-ready library.
API could and will change. App will crash (there are some untested unsafe blocks), some
promises will silently drop (there are stil no unit tests), documentation is incomplete
and so on. But. But. Examples works like a charm. And this fact gives us a lot of hope.

## License

The `pecs` is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

This means you can select the license you prefer!
This dual-licensing approach is the de-facto standard in the Rust ecosystem and there are [very good reasons](https://github.com/bevyengine/bevy/issues/2373) to include both.
