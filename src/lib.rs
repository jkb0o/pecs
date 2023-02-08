pub mod prelude {
    pub use bevy_promise_core::PromisePlugin;
    pub use bevy_promise_core::Promise;

    pub use bevy_promise_core::timer::TimerOpsExtension;
    pub use bevy_promise_macro::promise;
}

pub use bevy_promise_core as core;