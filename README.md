[![crates.io](https://img.shields.io/crates/v/pecs)](https://crates.io/crates/pecs)
[![MIT/Apache 2.0](https://img.shields.io/badge/license-MIT%2FApache-blue.svg)](https://github.com/jkb0o/pecs#license)
[![Bevy tracking](https://img.shields.io/badge/bevy-0.11-lightblue)](https://github.com/bevyengine/bevy/releases/tag/v0.11.0)
[![docs.rs](https://docs.rs/pecs/badge.svg)](https://docs.rs/pecs)


## About
`pecs` is a plugin for [Bevy](https://bevyengine.org) that allows you to execute code asynchronously
by chaining multiple promises as part of Bevy's `ecs` environment.

`pecs` stands for `Promise Entity Component System`.

Resources:
- [Docs](https://docs.rs/pecs/)
- [Examples](https://github.com/jkb0o/pecs/tree/master/examples)
- [Report an issue](https://github.com/jkb0o/pecs/issues/new)
- [Provide an idea](https://github.com/jkb0o/pecs/issues/new)

Compatibility:
| bevy | pecs |
|------|------|
| 0.12 | 0.5  |
| 0.11 | 0.4  |
| 0.10 | 0.3  |
| 0.9  | 0.2  |

### Features
- Promise chaining with `then()`/`then_repeat()`
- State passing (`state` for promises is like `self` for items).
- Complete type inference (the next promise knows the type of the previous result).
- Out-of-the-box timer, UI and HTTP promises via stateless `asyn` mod and
  stateful  `state.asyn()` method.
- Custom promise registration (add any asynchronous function you want!).
- [System parameters](https://docs.rs/bevy/latest/bevy/ecs/system/trait.SystemParam.html) fetching
  (promise `asyn!` functions accept the same parameters as Bevy systems do).
- Nested promises (with chaining, obviously).
- Combining promises with `any/all` for tuple/vec of promises via stateless `Promise::any()`
  /`Promise::all()` methods or stateful `state.any()`/`state.all()` methods.
- State mapping via `with(value)`/`map(func)` (changes state type/value over chain calls).
- Result mapping via `with_result(value)`/`map_result(func)` (changes result type/value over chain calls).

## Example
There is the output of the above example, pay some attention to time stamps:
```text
18.667 INFO bevy_render::renderer: AdapterInfo { ... }
18.835 INFO simple: Wait a second..
19.842 INFO simple: How large is is the Bevy main web page?
19.924 INFO simple: It is 17759 bytes!
19.924 INFO simple: It tooks 1.09s to do this job.
19.924 INFO simple: Exiting now
```

## Work in Progress
This crate is pretty young. API could and will change. The app may crash. Some
promises could silently drop. Documentation is incomplete.

But. But. Examples work like a charm. And this fact gives us a lot of hope.


## License

The `pecs` is dual-licensed under either:

- MIT License ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))

This means you can select the license you prefer!
This dual-licensing approach is the de-facto standard in the Rust ecosystem and there are [very good reasons](https://github.com/bevyengine/bevy/issues/2373) to include both.
