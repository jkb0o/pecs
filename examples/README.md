### About

This is the page with the described examples.

> **Note**
> 
> Make sure you are inspecting examples for right version.
> 
> There is the examples page for stable (published on crates.io) version:
> 
> https://github.com/jkb0o/pecs/tree/stable/examples


| Example                            | How to run                             | Description                                      |
|------------------------------------|----------------------------------------|--------------------------------------------------|
| [simple](simple.rs)                | `cargo run --example simple`           | Chain promises, defer some call using `asyn::timeout(sec)`, make http requests with `asyn::http::get()`.
| [custom_timer](custom_timer.rs)    | `cargo run --example custom_timer`     | Create custom promises, resolve them, promise <-> ecs relations workout.
| [combind_vecs](combine_vecs.rs)    | `cargo run --example combine_vecs`     | Combine promises: wait all/any of `Vec<Promise>` to resolve, iterator extension, state passing.
| [complex](complex.rs)              | `cargo run --example complex`          | Every new feature added to this example. It contains almost everything what `pecs` provides.
