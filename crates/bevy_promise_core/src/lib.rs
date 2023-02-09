use std::{
    any::type_name,
    cell::RefCell,
    mem,
    sync::{Arc, RwLock},
    thread::{self, ThreadId},
};

use bevy::{ecs::system::Command, prelude::*, utils::HashMap};

pub struct AsyncOps<T>(pub T);
pub struct AsyncState<T>(pub PromiseState<T>);
pub struct AsyncValue<T>(pub T);

pub fn promise_resolve<R: 'static, E: 'static, S: 'static>(
    world: &mut World,
    id: PromiseId,
    result: R,
    state: S,
) {
    // info!("rejecting {id}")
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    match {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        (
            mem::take(&mut prom.resolve),
            mem::take(&mut prom.resolve_reject),
        )
    } {
        (Some(resolve), None) => resolve(world, result, state),
        (None, Some(resolve_reject)) => resolve_reject(world, Ok(result), state),
        (None, None) => {}
        _ => error!("Misconfigured promise"),
    }
    registry.0.write().unwrap().remove(&id);
    // info!(
    //     "resolved {id}<{}, {}, {}> ({} left)",
    //     type_name::<R>(),
    //     type_name::<E>(),
    //     type_name::<S>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub fn promise_reject<R: 'static, E: 'static, S: 'static>(
    world: &mut World,
    id: PromiseId,
    error: E,
    state: S,
) {
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    match {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        let (r, rr, hr) = (
            mem::take(&mut prom.reject),
            mem::take(&mut prom.resolve_reject),
            prom.resolve.is_some(),
        );
        (r, rr, hr)
    } {
        (Some(reject), None, _) => reject(world, error, state),
        (None, Some(resolve_reject), _) => resolve_reject(world, Err(error), state),
        (None, None, p) if p => {
            warn!("Discarding resolve branch of {id}<{}, {}, {}>: missed reject handler while promise rejected.", type_name::<R>(), type_name::<E>(), type_name::<S>());
            let mut write = registry.0.write().unwrap();
            let prom = write.get_mut(&id).unwrap();
            if let Some(discard) = mem::take(&mut prom.discard) {
                discard(world, id)
            }
        }
        _ => error!("Misconfigured promise"),
    }
    registry.0.write().unwrap().remove(&id);
    // info!(
    //     "rejected {id}<{}, {}, {}> ({} left)",
    //     type_name::<R>(),
    //     type_name::<E>(),
    //     type_name::<S>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub fn promise_register<R: 'static, E: 'static, S: 'static>(
    world: &mut World,
    mut promise: Promise<R, E, S>,
) {
    let id = promise.id;
    // info!("registering {id}");
    let register = promise.register;
    promise.register = None;
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    registry.0.write().unwrap().insert(id, promise);
    if let Some(register) = register {
        register(world, id)
    }
    // info!(
    //     "registered {id}<{}, {}, {}> ({} left)",
    //     type_name::<R>(),
    //     type_name::<E>(),
    //     type_name::<S>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub fn promise_discard<R: 'static, E: 'static, S: 'static>(world: &mut World, id: PromiseId) {
    // info!("discarding {id}");
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    if let Some(discard) = {
        let mut write = registry.0.write().unwrap();
        if let Some(prom) = write.get_mut(&id) {
            mem::take(&mut prom.discard)
        } else {
            error!(
                "Internal promise error: trying to discard complete {id}<{}, {}, {}>",
                type_name::<R>(),
                type_name::<E>(),
                type_name::<S>(),
            );
            None
        }
    } {
        discard(world, id);
    }
    registry.0.write().unwrap().remove(&id);
    // info!(
    //     "discarded {id}<{}, {}, {}> ({} left)",
    //     type_name::<R>(),
    //     type_name::<E>(),
    //     type_name::<S>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub struct Promise<R, E, S> {
    id: PromiseId,
    register: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    discard: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    resolve: Option<Box<dyn FnOnce(&mut World, R, S)>>,
    reject: Option<Box<dyn FnOnce(&mut World, E, S)>>,
    resolve_reject: Option<Box<dyn FnOnce(&mut World, Result<R, E>, S)>>,
}
unsafe impl<R, E, S> Send for Promise<R, E, S> {}
unsafe impl<R, E, S> Sync for Promise<R, E, S> {}

pub enum PromiseResult<R, E, S> {
    Resolve(R, S),
    Rejected(E, S),
    Await(Promise<R, E, S>),
}

pub struct OnceValue<T>(*mut T);
unsafe impl<T> Send for OnceValue<T> {}
unsafe impl<T> Sync for OnceValue<T> {}
impl<T> OnceValue<T> {
    pub fn new(value: T) -> OnceValue<T> {
        let b = Box::new(value);
        OnceValue(Box::leak(b) as *mut T)
    }
    pub fn get(&self) -> T {
        if self.0.is_null() {
            panic!("Ups.")
        }
        let b = unsafe { Box::from_raw(self.0) };
        *b
    }
}

impl<R, E, S> From<Promise<R, E, S>> for PromiseResult<R, E, S> {
    fn from(value: Promise<R, E, S>) -> Self {
        PromiseResult::Await(value)
    }
}

#[derive(Resource)]
struct PromiseRegistry<R, E, S>(Arc<RwLock<HashMap<PromiseId, Promise<R, E, S>>>>);
impl<R, E, S> Default for PromiseRegistry<R, E, S> {
    fn default() -> Self {
        PromiseRegistry(Arc::new(RwLock::new(HashMap::new())))
    }
}
impl<R, E, S> Clone for PromiseRegistry<R, E, S> {
    fn clone(&self) -> Self {
        PromiseRegistry(self.0.clone())
    }
}

thread_local!(static PROMISE_LOCAL_ID: std::cell::RefCell<usize>  = RefCell::new(0));
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromiseId {
    thread: ThreadId,
    local: usize,
}

impl std::fmt::Display for PromiseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = format!("{:?}", self.thread);
        write!(
            f,
            "Promise({}:{})",
            t.strip_prefix("ThreadId(")
                .unwrap()
                .strip_suffix(")")
                .unwrap(),
            self.local
        )
    }
}

impl std::fmt::Debug for PromiseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

// impl <R: 'static, E: 'static> Promise<R, E> {
// }
impl<E: 'static> Promise<(), E, ()> {
    #[allow(non_snake_case)]
    pub fn Err(err: E) -> PromiseResult<(), E, ()> {
        PromiseResult::Rejected(err, ())
    }
}
impl<R: 'static> Promise<R, (), ()> {
    #[allow(non_snake_case)]
    pub fn Ok(result: R) -> PromiseResult<R, (), ()> {
        PromiseResult::Resolve(result, ())
    }
}
impl<R: 'static, E: 'static> Promise<R, E, ()> {
    #[allow(non_snake_case)]
    pub fn Resolve(result: R) -> PromiseResult<R, E, ()> {
        PromiseResult::Resolve(result, ())
    }
    #[allow(non_snake_case)]
    pub fn Reject(error: E) -> PromiseResult<R, E, ()> {
        PromiseResult::Rejected(error, ())
    }
    #[allow(non_snake_case)]
    pub fn Await(promise: Promise<R, E, ()>) -> PromiseResult<R, E, ()> {
        PromiseResult::Await(promise)
    }
}
impl<R: 'static, E: 'static, S: 'static> Promise<R, E, S> {
    pub fn id() -> PromiseId {
        PROMISE_LOCAL_ID.with(|id| {
            let mut new_id = id.borrow_mut();
            *new_id += 1;
            PromiseId {
                thread: thread::current().id(),
                local: *new_id,
            }
        })
    }

    pub fn new<
        Params,
        D: 'static,
        P: Into<PromiseResult<R, E, S>>,
        F: 'static + IntoSystem<AsyncState<D>, P, Params>,
    >(
        default_state: D,
        func: F,
    ) -> Promise<R, E, S> {
        let id = Self::id();
        // let default = OnceValue::new(default_state);
        Promise {
            id,
            resolve: None,
            reject: None,
            resolve_reject: None,
            discard: None,
            register: Some(Box::new(move |world, id| {
                let mut system = IntoSystem::into_system(func);
                system.initialize(world);
                let pr = system
                    .run(AsyncState(PromiseState::new(default_state)), world)
                    .into();
                system.apply_buffers(world);
                match pr {
                    PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S>(world, id, r, c),
                    PromiseResult::Rejected(e, c) => promise_reject::<R, E, S>(world, id, e, c),
                    PromiseResult::Await(mut p) => {
                        if p.resolve.is_some() || p.resolve_reject.is_some() {
                            error!(
                                "Misconfigured {}<{}, {}, {}>, resolve already defined",
                                p.id,
                                type_name::<R>(),
                                type_name::<E>(),
                                type_name::<S>()
                            );
                            return;
                        }
                        p.resolve = Some(Box::new(move |world, r, c| {
                            promise_resolve::<R, E, S>(world, id, r, c)
                        }));
                        promise_register::<R, E, S>(world, p);
                    }
                }
            })),
        }
    }

    pub fn register<
        F: 'static + FnOnce(&mut World, PromiseId),
        D: 'static + FnOnce(&mut World, PromiseId),
    >(
        register: F,
        discard: D,
    ) -> Promise<R, E, S> {
        Promise {
            id: Self::id(),
            resolve: None,
            reject: None,
            resolve_reject: None,
            register: Some(Box::new(register)),
            discard: Some(Box::new(discard)),
        }
    }

    pub fn then<
        Params,
        R2: 'static,
        E2: 'static,
        C2: 'static,
        P: Into<PromiseResult<R2, E2, C2>>,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<R>), P, Params>,
    >(
        mut self,
        func: F,
    ) -> Promise<R2, E2, C2> {
        let id = Self::id();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            if let Some(discard) = discard {
                discard(world, self_id);
            }
            promise_discard::<R2, E2, C2>(world, id);
        }));
        self.resolve = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system
                .run(
                    (AsyncState(PromiseState::new(state)), AsyncValue(result)),
                    world,
                )
                .into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R2, E2, C2>(world, id, r, c),
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E2, C2>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}>, resolve already defined",
                            p.id,
                            type_name::<R2>(),
                            type_name::<E2>()
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R2, E2, C2>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R2, E2, C2>(world, id, e, c);
                    }));
                    promise_register::<R2, E2, C2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, S>(world, self)
            })),
            discard: None,
            resolve_reject: None,
            resolve: None,
            reject: None,
        }
    }

    pub fn then_catch<
        Params,
        R2: 'static,
        E2: 'static,
        C2: 'static,
        P: Into<PromiseResult<R2, E2, C2>>,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<Result<R, E>>), P, Params>,
    >(
        mut self,
        func: F,
    ) -> Promise<R2, E2, C2> {
        let id = Self::id();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            if let Some(discard) = discard {
                discard(world, self_id);
            }
            promise_discard::<R2, E2, C2>(world, id);
        }));
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system
                .run(
                    (AsyncState(PromiseState::new(state)), AsyncValue(result)),
                    world,
                )
                .into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R2, E2, C2>(world, id, r, c),
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E2, C2>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}>, resolve/reject already defined",
                            p.id,
                            type_name::<R2>(),
                            type_name::<E2>()
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R2, E2, C2>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R2, E2, C2>(world, id, e, c);
                    }));
                    promise_register::<R2, E2, C2>(world, p);
                }
            }
        }));
        // self.then(move |In((AsyncState(c), AsyncValue(r)))| {
        //     PromiseResult::<R, (), C>::Resolve(r, c.0)
        // })
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, S>(world, self);
            })),
            discard: None,
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }

    pub fn catch<
        Params,
        P: Into<PromiseResult<R, (), S>>,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<E>), P, Params>,
    >(
        mut self,
        func: F,
    ) -> Promise<R, (), S> {
        let id = self.id;
        self.reject = Some(Box::new(move |world, error, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system
                .run(
                    (AsyncState(PromiseState { value: state }), AsyncValue(error)),
                    world,
                )
                .into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S>(world, id, r, c),
                PromiseResult::Rejected(_, _) => {
                    error!(
                        "Misconfigured {id}<{}, {}, {}>: catch exit with empty (dropped) error",
                        type_name::<R>(),
                        type_name::<E>(),
                        type_name::<E>(),
                    );
                }
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {id}<{}, {}, {}>: resolve already defined",
                            type_name::<R>(),
                            type_name::<E>(),
                            type_name::<S>(),
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R, E, S>(world, id, r, c);
                    }));
                }
            };
        }));
        self.then(move |In((AsyncState(c), AsyncValue(r)))| {
            PromiseResult::<R, (), S>::Resolve(r, c.value)
        })
    }

    pub fn map_state<S2: 'static, F: 'static + FnOnce(S) -> S2>(self, map: F) -> Promise<R, E, S2> {
        let map = OnceValue::new(map);
        self.then_catch(move |In((AsyncState(s), AsyncValue(r)))| {
            PromiseState::new(map.get()(s.value)).result(r)
        })
    }
    pub fn map<R2: 'static, F: 'static + FnOnce(R) -> R2>(self, map: F) -> Promise<R2, E, S> {
        let map = OnceValue::new(map);
        self.then_catch(move |In((AsyncState(s), AsyncValue(r)))| match r {
            Ok(r) => s.resolve(map.get()(r)),
            Err(e) => s.reject(e),
        })
    }
    pub fn map_error<E2: 'static, F: 'static + FnOnce(E) -> E2>(self, map: F) -> Promise<R, E2, S> {
        let map = OnceValue::new(map);
        self.then_catch(move |In((AsyncState(s), AsyncValue(r)))| match r {
            Ok(r) => s.resolve(r),
            Err(e) => s.reject(map.get()(e)),
        })
    }
}

pub struct PromiseCommand<R, E> {
    id: PromiseId,
    result: Result<R, E>,
}

impl<R> PromiseCommand<R, ()> {
    pub fn ok(id: PromiseId, result: R) -> Self {
        PromiseCommand {
            id,
            result: Ok(result),
        }
    }
}

impl<R, E> PromiseCommand<R, E> {
    pub fn resolve(id: PromiseId, result: R) -> Self {
        Self {
            id,
            result: Ok(result),
        }
    }
    pub fn reject(id: PromiseId, error: E) -> Self {
        Self {
            id,
            result: Err(error),
        }
    }
    pub fn result(id: PromiseId, result: Result<R, E>) -> Self {
        Self { id, result }
    }
}

impl<R: 'static + Send + Sync, E: 'static + Send + Sync> Command for PromiseCommand<R, E> {
    fn write(self, world: &mut World) {
        match self.result {
            Ok(r) => promise_resolve::<R, E, ()>(world, self.id, r, ()),
            Err(e) => promise_reject::<R, E, ()>(world, self.id, e, ()),
        }
    }
}

impl<R: 'static, E: 'static, S: 'static> Command for Promise<R, E, S> {
    fn write(self, world: &mut World) {
        promise_register::<R, E, S>(world, self)
    }
}

pub mod timer {
    use super::*;
    pub fn timeout(duration: f32) -> Promise<(), (), ()> {
        Promise::<(), (), ()>::register(
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
    pub struct Timer<T>(T);
    impl<T: 'static> Timer<T> {
        pub fn delay(self, duration: f32) -> Promise<(), (), T> {
            timeout(duration).map_state(|_| self.0)
        }
    }
    pub trait TimerOpsExtension<T> {
        fn timer(self) -> Timer<T>;
    }
    impl<T> TimerOpsExtension<T> for AsyncOps<T> {
        fn timer(self) -> Timer<T> {
            Timer(self.0)
        }
    }

    #[derive(Resource, Deref, DerefMut, Default)]
    pub struct Timers(HashMap<PromiseId, f32>);

    pub fn process_timers(time: Res<Time>, mut commands: Commands, mut timers: ResMut<Timers>) {
        let elapsed = time.elapsed_seconds();
        timers.drain_filter(|promise, end| {
            if &elapsed >= end {
                commands.add(PromiseCommand::ok(*promise, ()));
                true
            } else {
                false
            }
        });
    }
}

pub struct PromiseState<S> {
    pub value: S,
}
impl<S: 'static> PromiseState<S> {
    pub fn new(value: S) -> PromiseState<S> {
        PromiseState { value }
    }
    pub fn cmd(self) -> AsyncOps<S> {
        AsyncOps(self.value)
    }
    pub fn ops(self) -> AsyncOps<S> {
        AsyncOps(self.value)
    }
    pub fn resolve<R, E>(self, result: R) -> PromiseResult<R, E, S> {
        PromiseResult::Resolve(result, self.value)
    }
    pub fn ok<R>(self, result: R) -> PromiseResult<R, (), S> {
        PromiseResult::Resolve(result, self.value)
    }
    pub fn err<E>(self, error: E) -> PromiseResult<(), E, S> {
        PromiseResult::Rejected(error, self.value)
    }
    pub fn reject<R, E>(self, error: E) -> PromiseResult<R, E, S> {
        PromiseResult::Rejected(error, self.value)
    }
    pub fn result<R, E>(self, result: Result<R, E>) -> PromiseResult<R, E, S> {
        match result {
            Ok(r) => PromiseResult::Resolve(r, self.value),
            Err(e) => PromiseResult::Rejected(e, self.value),
        }
    }
    pub fn with<T, F: FnOnce(S) -> T>(self, map: F) -> PromiseState<T> {
        PromiseState {
            value: map(self.value),
        }
    }

    pub fn then<R: 'static, E: 'static, S2: 'static>(
        self,
        promise: Promise<R, E, S2>,
    ) -> Promise<R, E, S> {
        promise.map_state(|_| self.value)
    }
}

impl<T: std::fmt::Display> std::fmt::Display for PromiseState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PromiseState({})", self.value)
    }
}

impl<T: std::fmt::Debug> std::fmt::Debug for PromiseState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PromiseState({:?})", self.value)
    }
}
