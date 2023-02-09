pub mod prelude {
    pub use bevy_promise_core::Promise;

    pub use bevy_promise_core::timer::TimerOpsExtension;
    pub use bevy_promise_macro::promise;

    use bevy::prelude::*;
    pub struct PromisePlugin;
    impl Plugin for PromisePlugin {
        fn build(&self, app: &mut App) {
            app.init_resource::<super::timer::Timers>();
            app.add_system(super::timer::process_timers);

            app.add_plugin(super::http::PromiseHttpPlugin);
        }
    }
}

pub use bevy_promise_core as core;
pub use bevy_promise_core::timer;
pub use bevy_promise_http as http;
