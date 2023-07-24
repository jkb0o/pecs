//! Defers promise resolving for a fixed amount of time
use super::*;
pub fn timeout(duration: f32) -> Promise<(), ()> {
    Promise::<(), ()>::register(
        move |world, id| {
            let time = world.resource::<Time>();
            let end = time.elapsed_seconds() + duration - time.delta_seconds();
            world.resource_mut::<Timers>().insert(id, end);
        },
        move |world, id| {
            world.resource_mut::<Timers>().remove(&id);
        },
    )
}
pub trait TimerOpsExtension<S> {
    fn timeout(self, duration: f32) -> Promise<S, ()>;
}
impl<S: 'static> TimerOpsExtension<S> for AsynOps<S> {
    fn timeout(self, duration: f32) -> Promise<S, ()> {
        timeout(duration).map(|_| self.0)
    }
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct Timers(HashMap<PromiseId, f32>);

pub fn process_timers(time: Res<Time>, mut commands: Commands, mut timers: ResMut<Timers>) {
    let elapsed = time.elapsed_seconds();
    timers.retain(|promise, end| {
        if &elapsed >= end {
            commands.add(PromiseCommand::resolve(*promise, ()));
            false
        } else {
            true
        }
    });
}
