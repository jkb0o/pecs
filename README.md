## About
`pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you
to execute code asynchronously by chaining multple promises with respect
of Bevy's `ecs` enviroment.

`pecs` is acronim for `Promise Entity Component System`.

Resources:
- [Docs](..)
- [Examples](..)
- [Report an issue](..)
- [Provide an idea](..)

### Features
- promise chaining with `then()`, `ok_then()` or `or_else()`
- state passing (`state` for promises is like `self` for items)
- complete type inference (the next promise knows tye types of previous result)
- out-of-the-box timer and http promises via `asyn` mod and stateful `state.asyn()`
- custom promise registretion (add any asyn function you want!)
- `system params` fetching (promise `asyn!` funcs acccepts the same params
  the bevy systems does)
- nested promises (with chaining, obviously)
- combining promises with any/all for tuple/vec of promises via stateles
  `Promise::any()`/`Promise::all()` or stateful `state.any()`/`state.all()`
- state mapping via `with(value)`/`map(func)` (change state type/value over chain calls)
- result mapping via `with_ok(value)`/`map_ok(func)` (change Ok type/value over chain calls)
- error mapping via `with_err(value)`/`map_err(func)` (change Err type over chain calls)

## Example
```rust
use bevy::{prelude::*, app::AppExit};
use pecs::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugin(PromisePlugin)
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
            info!("Looks like I need to know how large is the Bevy main web page!");
            state.asyn().http().get("https://bevyengine.org").send()
        }))
        .then(asyn!(state, result => {
            match result {
                Ok(response) => info!("It is {} bytes!", response.bytes.len()),
                Err(err) => info!("Ahhh... something goes wrong: {err}")
            }
            state.done()
        }))
        .then(asyn!(state, _, time: Res<Time>, mut exit: EventWriter<AppExit> => {
            let duration = time.elapsed_seconds() - state.value;
            info!("It tooks {duration:0.2}s to do this job.");
            info!("Exiting now");
            exit.send(AppExit);
            state.done()
        }))
    );
}
```
There is otput of the above example, pay some attention to time stamps:
```text
15:52:20.459635Z  INFO bevy_render::renderer: AdapterInfo { ... }
15:52:20.643082Z  INFO simple: Wait a second..
15:52:21.659898Z  INFO simple: Looks like I need to know how large is the Bevy main web page!
15:52:21.775228Z  INFO simple: It is 17759 bytes!
15:52:21.775319Z  INFO simple: It tooks 1.13s to do this job.
15:52:21.775342Z  INFO simple: Exiting now
```

## Work in Progress

This repo is more like an experimental-proof-of-concept then a production-ready library.
API could and will change. App will crash (there are some untested unsafe blocks), some
promises will silently drop (there are stil no unit tests), documentation is incomplete
and so on. But. But. Examples works like a charm. And this fact gives us a lot of hope.

## License

The `belly` is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

This means you can select the license you prefer!
This dual-licensing approach is the de-facto standard in the Rust ecosystem and there are [very good reasons](https://github.com/bevyengine/bevy/issues/2373) to include both.