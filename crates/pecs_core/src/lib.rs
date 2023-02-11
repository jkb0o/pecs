//! Core [`Promise`] functionality.
use bevy::{
    ecs::system::{Command, SystemParam, SystemParamItem},
    prelude::*,
    utils::HashMap,
};
use pecs_macro::{asyn, impl_all_promises, impl_any_promises};
use std::{
    any::type_name,
    cell::RefCell,
    marker::PhantomData,
    mem,
    sync::{Arc, RwLock},
    thread::{self, ThreadId},
};

pub struct AsyncOps<T>(pub T);

pub fn promise_resolve<R: 'static, E: 'static, S: 'static>(world: &mut World, id: PromiseId, result: R, state: S) {
    // info!(
    //     "resolving {id}<{}, {}, {}>",
    //     type_name::<R>(),
    //     type_name::<E>(),
    //     type_name::<S>(),
    // );
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    match {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        (mem::take(&mut prom.resolve), mem::take(&mut prom.resolve_reject))
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

pub fn promise_reject<R: 'static, E: 'static, S: 'static>(world: &mut World, id: PromiseId, error: E, state: S) {
    // info!("rejecting {id}");
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
            warn!(
                "Discarding resolve branch of {id}<{}, {}, {}>: missed reject handler while promise rejected.",
                type_name::<R>(),
                type_name::<E>(),
                type_name::<S>()
            );
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

pub fn promise_register<R: 'static, E: 'static, S: 'static>(world: &mut World, mut promise: Promise<R, E, S>) {
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

pub trait PromiseParams: 'static + SystemParam + Send + Sync {}
impl<T: 'static + SystemParam + Send + Sync> PromiseParams for T {}

pub struct AsynFunction<Input, Output, Params: PromiseParams> {
    pub marker: PhantomData<Params>,
    pub body: fn(In<Input>, SystemParamItem<Params>) -> Output,
}
impl<Input, Output, Params: PromiseParams> AsynFunction<Input, Output, Params> {
    fn new(body: fn(In<Input>, SystemParamItem<Params>) -> Output) -> Self {
        AsynFunction {
            body,
            marker: PhantomData,
        }
    }
}

thread_local!(static PROMISE_LOCAL_ID: std::cell::RefCell<usize>  = RefCell::new(0));
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct PromiseId {
    thread: ThreadId,
    local: usize,
}
impl PromiseId {
    pub fn new() -> PromiseId {
        PROMISE_LOCAL_ID.with(|id| {
            let mut new_id = id.borrow_mut();
            *new_id += 1;
            PromiseId {
                thread: thread::current().id(),
                local: *new_id,
            }
        })
    }
}

impl std::fmt::Display for PromiseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let t = format!("{:?}", self.thread);
        write!(
            f,
            "Promise({}:{})",
            t.strip_prefix("ThreadId(").unwrap().strip_suffix(")").unwrap(),
            self.local
        )
    }
}

impl std::fmt::Debug for PromiseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self}")
    }
}

pub enum PromiseResult<R, E, S> {
    Resolve(R, S),
    Reject(E, S),
    Await(Promise<R, E, S>),
}

impl<R, E, S> From<Promise<R, E, S>> for PromiseResult<R, E, S> {
    fn from(value: Promise<R, E, S>) -> Self {
        PromiseResult::Await(value)
    }
}

impl<R, E, S> From<IncompletePromise<R, E, S>> for PromiseResult<R, E, S> {
    fn from(value: IncompletePromise<R, E, S>) -> Self {
        PromiseResult::Await(value.0)
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

impl<R: 'static, E: 'static, S: 'static> Promise<R, E, S> {
    /// Create new [`Promise`] with empty [state][PromiseState]
    /// ```
    /// # use bevy::prelude::*
    /// fn setup(mut commands: Commands) {
    ///     commands.add(
    ///         // type of `s` is infered as `PromiseState<()>`
    ///         Promise::start(asyn!(s => {
    ///             info!("I'm new Promise with empty state");
    ///             s.ok(())
    ///         }))
    ///     );
    /// }
    /// ```
    pub fn start<Params: PromiseParams, P: 'static + Into<PromiseResult<R, E, S>>>(
        func: AsynFunction<PromiseState<()>, P, Params>,
    ) -> Promise<R, E, S> {
        Promise::new((), func)
    }
    /// Create new [`Promise`] with [`PromiseState<D>`] state.
    /// ```
    /// # use bevy::prelude::*
    /// fn setup(mut commands: Commands) {
    ///     let entity = commands.spawn_empty().id();
    ///     commands.add(
    ///         // type of `s` is infered as `PromiseState<Entity>`
    ///         Promise::new(entity, asyn!(s => {
    ///             info!("I'm started with some entity {:?}", s.value);
    ///             s.ok(())
    ///         }))
    ///     );
    /// }
    /// ```
    pub fn new<D: 'static, Params: PromiseParams, P: 'static + Into<PromiseResult<R, E, S>>>(
        default_state: D,
        func: AsynFunction<PromiseState<D>, P, Params>,
    ) -> Promise<R, E, S> {
        let id = PromiseId::new();
        // let default = OnceValue::new(default_state);
        Promise {
            id,
            resolve: None,
            reject: None,
            resolve_reject: None,
            discard: None,
            register: Some(Box::new(move |world, id| {
                let mut system = IntoSystem::into_system(func.body);
                system.initialize(world);
                let pr = system.run(PromiseState::new(default_state), world).into();
                system.apply_buffers(world);
                match pr {
                    PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S>(world, id, r, c),
                    PromiseResult::Reject(e, c) => promise_reject::<R, E, S>(world, id, e, c),
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
                        p.resolve = Some(Box::new(move |world, r, c| promise_resolve::<R, E, S>(world, id, r, c)));
                        promise_register::<R, E, S>(world, p);
                    }
                }
            })),
        }
    }

    /// Create new [`Promise`] with resolve/reject behaviour controlled by user.
    /// It takes two closures as arguments: `invoke` and `discard`. The
    /// `invoke` will be executed when the promise's turn comes. The discard
    /// will be called in the case of promise cancellation. Both closures takes
    /// [`&mut World`][bevy::prelude::World] and [`PromiseId`] as argiments.
    /// ```rust
    /// #[derive(Component)]
    /// /// Holds PromiseId and the time when the timer should time out.
    /// pub struct MyTimer(PromiseId, f32);
    ///
    /// /// creates promise that will resolve after [`duration`] seconds
    /// pub fn delay(duration: f32) -> Promise<(), (), ()> {
    ///     Promise::register(
    ///         // this will be invoked when promise's turn comes
    ///         move |world, id| {
    ///             let now = world.resource::<Time>().elapsed_seconds();
    ///             // store timer
    ///             world.spawn(MyTimer(id, now + duration));
    ///         },
    ///         // this will be invoked when promise got discarded
    ///         move |world, id| {
    ///             let entity = {
    ///                 let mut timers = world.query::<(Entity, &MyTimer)>();
    ///                 timers.iter(world)
    ///                     .filter(|(_entity, timer)| timer.0 == id )
    ///                     .map(|(entity, _timer)| entity)
    ///                     .next()
    ///             };
    ///             if let Some(entity) = entity {
    ///                 world.despawn(entity);
    ///             }
    ///         }
    ///     )
    /// }
    ///
    /// /// iterate ofver all timers and resolves completed
    /// pub fn process_timers_system(
    ///     timers: Query<(Entity, &MyTimer)>,
    ///     mut commands: Commands,
    ///     time: Res<Time>,
    /// ) {
    ///     let now = time.elapsed_seconds();
    ///     for (entity, timer) in timers.iter().filter(|(_, t)| t.1 < now) {
    ///         let promise_id = timer.0;
    ///         commands.promise(promise_id).ok(());
    ///         commands.entity(entity).despawn();
    ///     }
    /// }
    ///
    /// fn setup(mut commands: Commands) {
    ///     // `delay()` can be called from inside promise
    ///     commands.add(
    ///         Promise::start(asyn!(_state => {
    ///             info!("Starting");
    ///             delay(1.)
    ///         })).then(asyn!(s, _ => {
    ///             info!("Completing");
    ///             s.done()
    ///         }))
    ///     );
    ///
    ///     // or queued directly to Commands
    ///     commands.add(
    ///         delay(2.)
    ///         .then(asyn!(s, _ => {
    ///             info!("I'm another timer");
    ///             s.done()
    ///         }))
    ///     );
    /// }
    /// ```
    pub fn register<F: 'static + FnOnce(&mut World, PromiseId), D: 'static + FnOnce(&mut World, PromiseId)>(
        invoke: F,
        discard: D,
    ) -> Promise<R, E, S> {
        Promise {
            id: PromiseId::new(),
            resolve: None,
            reject: None,
            resolve_reject: None,
            register: Some(Box::new(invoke)),
            discard: Some(Box::new(discard)),
        }
    }

    pub fn ok_then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        mut self,
        func: AsynFunction<(PromiseState<S>, R), P, Params>,
    ) -> IncompletePromise<R2, E2, S2> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            if let Some(discard) = discard {
                discard(world, self_id);
            }
            promise_discard::<R2, E2, S2>(world, id);
        }));
        self.resolve = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func.body);
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R2, E2, S2>(world, id, r, c),
                PromiseResult::Reject(e, c) => promise_reject::<R2, E2, S2>(world, id, e, c),
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
                        promise_resolve::<R2, E2, S2>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R2, E2, S2>(world, id, e, c);
                    }));
                    promise_register::<R2, E2, S2>(world, p);
                }
            }
        }));
        IncompletePromise(Promise {
            id,
            register: Some(Box::new(move |world, _id| promise_register::<R, E, S>(world, self))),
            discard: None,
            resolve_reject: None,
            resolve: None,
            reject: None,
        })
    }

    pub fn then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        mut self,
        func: AsynFunction<(PromiseState<S>, Result<R, E>), P, Params>,
    ) -> Promise<R2, E2, S2> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R2, E2, S2>(world, id);
        }));
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func.body);
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R2, E2, S2>(world, id, r, c),
                PromiseResult::Reject(e, c) => promise_reject::<R2, E2, S2>(world, id, e, c),
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
                        promise_resolve::<R2, E2, S2>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R2, E2, S2>(world, id, e, c);
                    }));
                    promise_register::<R2, E2, S2>(world, p);
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
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }

    pub fn with_ok<R2: 'static>(self, value: R2) -> Promise<R2, E, S> {
        self.map_ok(|_| value)
    }

    pub fn map_ok<R2: 'static, F: 'static + FnOnce(R) -> R2>(mut self, map: F) -> Promise<R2, E, S> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R2, E, S>(world, id);
        }));
        let mut map = MutPtr::new(map);
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(move |In((s, r)): In<(PromiseState<S>, Result<R, E>)>| match r {
                Ok(r) => s.resolve(map.get()(r)),
                Err(e) => s.reject(e),
            });
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R2, E, S>(world, id, r, c),
                PromiseResult::Reject(e, c) => promise_reject::<R2, E, S>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}, {}>, resolve/reject already defined",
                            p.id,
                            type_name::<R2>(),
                            type_name::<E>(),
                            type_name::<S>(),
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R2, E, S>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R2, E, S>(world, id, e, c);
                    }));
                    promise_register::<R2, E, S>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, S>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }

    pub fn with_err<E2: 'static>(self, error: E2) -> Promise<R, E2, S> {
        self.map_err(|_| error)
    }
    pub fn map_err<E2: 'static, F: 'static + FnOnce(E) -> E2>(mut self, map: F) -> Promise<R, E2, S> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R, E2, S>(world, id);
        }));
        let mut map = MutPtr::new(map);
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(move |In((s, r)): In<(PromiseState<S>, Result<R, E>)>| match r {
                Ok(r) => s.resolve(r),
                Err(e) => s.reject(map.get()(e)),
            });
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E2, S>(world, id, r, c),
                PromiseResult::Reject(e, c) => promise_reject::<R, E2, S>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}, {}>, resolve/reject already defined",
                            p.id,
                            type_name::<R>(),
                            type_name::<E2>(),
                            type_name::<S>(),
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R, E2, S>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R, E2, S>(world, id, e, c);
                    }));
                    promise_register::<R, E2, S>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, S>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }

    pub fn with<S2: 'static>(self, state: S2) -> Promise<R, E, S2> {
        self.map(|_| state)
    }
    pub fn map<S2: 'static, F: 'static + FnOnce(S) -> S2>(mut self, map: F) -> Promise<R, E, S2> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R, E, S2>(world, id);
        }));
        let mut map = MutPtr::new(map);
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(move |In((s, r)): In<(PromiseState<S>, Result<R, E>)>| {
                PromiseState::new(map.get()(s.value)).result(r)
            });
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S2>(world, id, r, c),
                PromiseResult::Reject(e, c) => promise_reject::<R, E, S2>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}, {}>, resolve/reject already defined",
                            p.id,
                            type_name::<R>(),
                            type_name::<E>(),
                            type_name::<S2>(),
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R, E, S2>(world, id, r, c);
                    }));
                    p.reject = Some(Box::new(move |world, e, c| {
                        promise_reject::<R, E, S2>(world, id, e, c);
                    }));
                    promise_register::<R, E, S2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, S>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }
}

impl<E: 'static> Promise<(), E, ()> {
    /// Create stateless [reject][PromiseResult::Reject] with error `E` and empty Ok.
    pub fn err(err: E) -> PromiseResult<(), E, ()> {
        PromiseResult::Reject(err, ())
    }
}
impl<R: 'static> Promise<R, (), ()> {
    /// Create stateless [resolve][PromiseResult::Resolve] with Ok `R` and empty Err.
    pub fn ok(result: R) -> PromiseResult<R, (), ()> {
        PromiseResult::Resolve(result, ())
    }
}
impl<R: 'static, E: 'static> Promise<R, E, ()> {
    /// Create stateless [resolve][PromiseResult::Resolve] with ok `R` and error `E`.
    /// `E` infered from context:
    /// ```rust
    /// if true {
    ///     // Err type is `'static str`
    ///     Promise::resolve(0.5)
    /// } else {
    ///     // Ok type is `f32`
    ///     Promise::reject("Oops")
    /// }
    /// ```
    /// Use [`Promise::ok`]  if you want to resolve with empty Err.
    pub fn resolve(result: R) -> PromiseResult<R, E, ()> {
        PromiseResult::Resolve(result, ())
    }
    /// Create stateless [reject][PromiseResult::Reject] with ok `R` and error `E`.
    /// `R` infered from context:
    /// ```rust
    /// if true {
    ///     // Err type is `'static str`
    ///     Promise::resolve(0.5)
    /// } else {
    ///     // Ok type is `f32`
    ///     Promise::reject("Oops")
    /// }
    /// ```
    /// Use [`Promise::err`]  if you want to reject with empty Ok.
    pub fn reject(error: E) -> PromiseResult<R, E, ()> {
        PromiseResult::Reject(error, ())
    }
}

// pub type AnyResult<A> = <A as AnyPromises>::Result;// as SystemParamFetch<'w, 's>>::Item;
// impl<R> Promise<AnyResult<R>, (), ()> {

// }
pub trait PromiseAnyMethod<T: AnyPromises> {
    fn any(any: T) -> Promise<T::Result, (), ()>;
}

impl<T: AnyPromises> PromiseAnyMethod<T> for Promise<T::Result, (), ()> {
    fn any(any: T) -> Promise<T::Result, (), ()> {
        any.register()
    }
}
pub trait PromiseAllMethod<T: AllPromises> {
    fn all(all: T) -> Promise<T::Result, (), ()>;
}

impl<T: AllPromises> PromiseAllMethod<T> for Promise<T::Result, (), ()> {
    fn all(all: T) -> Promise<T::Result, (), ()> {
        all.register()
    }
}
// pub fn any<A: AnyPromises>(self, any: A) -> Promise<A::Result, (), S> {

pub struct IncompletePromise<R, E, S>(Promise<R, E, S>);

impl<R: 'static, E: 'static, S: 'static> IncompletePromise<R, E, S> {
    pub fn or_else<Params: PromiseParams, P: 'static + Into<PromiseResult<R, (), S>>>(
        self,
        func: AsynFunction<(PromiseState<S>, E), P, Params>,
    ) -> Promise<R, (), S> {
        let mut this = self.0;
        let id = this.id;
        this.reject = Some(Box::new(move |world, error, state| {
            let mut system = IntoSystem::into_system(func.body);
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), error), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S>(world, id, r, c),
                PromiseResult::Reject(_, _) => {
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
        this.ok_then(asyn!(s, r => {
            PromiseResult::<R, (), S>::Resolve(r, s.value)
        }))
        .0
    }
    pub fn then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        self,
        func: AsynFunction<(PromiseState<S>, Result<R, E>), P, Params>,
    ) -> Promise<R2, E2, S2> {
        self.0.then(func)
    }
}

impl<R: 'static, S: 'static> IncompletePromise<R, (), S> {
    pub fn ok_then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        self,
        func: AsynFunction<(PromiseState<S>, R), P, Params>,
    ) -> IncompletePromise<R2, E2, S2> {
        self.0.ok_then(func)
    }
}

pub struct PromiseCommand<R, E> {
    id: PromiseId,
    result: Result<R, E>,
}

impl<R> PromiseCommand<R, ()> {
    pub fn ok(id: PromiseId, result: R) -> Self {
        PromiseCommand { id, result: Ok(result) }
    }
}

impl<E> PromiseCommand<(), E> {
    pub fn err(id: PromiseId, error: E) -> Self {
        PromiseCommand { id, result: Err(error) }
    }
}

impl<R, E> PromiseCommand<R, E> {
    pub fn resolve(id: PromiseId, result: R) -> Self {
        Self { id, result: Ok(result) }
    }
    pub fn reject(id: PromiseId, error: E) -> Self {
        Self { id, result: Err(error) }
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

impl<R: 'static, S: 'static> Command for Promise<R, (), S> {
    fn write(self, world: &mut World) {
        promise_register::<R, (), S>(world, self)
    }
}

impl<R: 'static, S: 'static> Command for IncompletePromise<R, (), S> {
    fn write(self, world: &mut World) {
        promise_register::<R, (), S>(world, self.0)
    }
}

pub struct PromiseCommands<'w, 's, 'a> {
    id: PromiseId,
    commands: &'a mut Commands<'w, 's>,
}
impl<'w, 's, 'a> PromiseCommands<'w, 's, 'a> {
    pub fn ok<R: 'static + Send + Sync>(&mut self, value: R) -> &mut Self {
        self.commands.add(PromiseCommand::ok(self.id, value));
        self
    }
    pub fn err<E: 'static + Send + Sync>(&mut self, error: E) -> &mut Self {
        self.commands.add(PromiseCommand::err(self.id, error));
        self
    }
    pub fn resolve<R: 'static + Send + Sync, E: 'static + Send + Sync>(&mut self, value: R) -> &mut Self {
        self.commands.add(PromiseCommand::<R, E>::resolve(self.id, value));
        self
    }
    pub fn reject<R: 'static + Send + Sync, E: 'static + Send + Sync>(&mut self, error: E) -> &mut Self {
        self.commands.add(PromiseCommand::<R, E>::reject(self.id, error));
        self
    }
    pub fn result<R: 'static + Send + Sync, E: 'static + Send + Sync>(&mut self, result: Result<R, E>) -> &mut Self {
        self.commands.add(PromiseCommand::result(self.id, result));
        self
    }
}

pub trait PromiseCommandsExtension<'w, 's> {
    fn promise<'a>(&'a mut self, id: PromiseId) -> PromiseCommands<'w, 's, 'a>;
}

impl<'w, 's> PromiseCommandsExtension<'w, 's> for Commands<'w, 's> {
    fn promise<'a>(&'a mut self, id: PromiseId) -> PromiseCommands<'w, 's, 'a> {
        PromiseCommands { id, commands: self }
    }
}

pub mod timer {
    //! Defers promise resolving for a fixed amount of time
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
    pub trait TimerOpsExtension<T> {
        fn timeout(self, duration: f32) -> Promise<(), (), T>;
    }
    impl<T: 'static> TimerOpsExtension<T> for AsyncOps<T> {
        fn timeout(self, duration: f32) -> Promise<(), (), T> {
            timeout(duration).map(|_| self.0)
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

impl<T: Clone> Clone for AsyncOps<T> {
    fn clone(&self) -> Self {
        AsyncOps(self.0.clone())
    }
}
impl<T: Copy> Copy for AsyncOps<T> {}
pub struct PromiseState<S> {
    pub value: S,
}
impl<S: 'static> PromiseState<S> {
    pub fn new(value: S) -> PromiseState<S> {
        PromiseState { value }
    }
    pub fn asyn(self) -> AsyncOps<S> {
        AsyncOps(self.value)
    }
    pub fn resolve<R, E>(self, result: R) -> PromiseResult<R, E, S> {
        PromiseResult::Resolve(result, self.value)
    }
    pub fn done(self) -> PromiseResult<(), (), S> {
        PromiseResult::Resolve((), self.value)
    }
    pub fn ok<R>(self, result: R) -> PromiseResult<R, (), S> {
        PromiseResult::Resolve(result, self.value)
    }
    pub fn err<E>(self, error: E) -> PromiseResult<(), E, S> {
        PromiseResult::Reject(error, self.value)
    }
    pub fn reject<R, E>(self, error: E) -> PromiseResult<R, E, S> {
        PromiseResult::Reject(error, self.value)
    }
    pub fn result<R, E>(self, result: Result<R, E>) -> PromiseResult<R, E, S> {
        match result {
            Ok(r) => PromiseResult::Resolve(r, self.value),
            Err(e) => PromiseResult::Reject(e, self.value),
        }
    }
    pub fn map<T, F: FnOnce(S) -> T>(self, map: F) -> PromiseState<T> {
        PromiseState { value: map(self.value) }
    }

    pub fn with<T: 'static>(self, value: T) -> PromiseState<T> {
        PromiseState { value }
    }

    pub fn then<R: 'static, E: 'static, S2: 'static>(self, promise: Promise<R, E, S2>) -> Promise<R, E, S> {
        promise.map(|_| self.value)
    }

    pub fn any<A: AnyPromises>(self, any: A) -> Promise<A::Result, (), S> {
        any.register().map(|_| self.value)
    }

    pub fn all<A: AllPromises>(self, all: A) -> Promise<A::Result, (), S> {
        all.register().map(|_| self.value)
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

pub struct MutPtr<T>(*mut T);
unsafe impl<T> Send for MutPtr<T> {}
unsafe impl<T> Sync for MutPtr<T> {}
impl<T> Clone for MutPtr<T> {
    fn clone(&self) -> Self {
        MutPtr(self.0)
    }
}

impl<T> MutPtr<T> {
    pub fn new(value: T) -> MutPtr<T> {
        let b = Box::new(value);
        MutPtr(Box::leak(b) as *mut T)
    }
    pub fn get(&mut self) -> T {
        if self.0.is_null() {
            panic!("Ups.")
        }
        let b = unsafe { Box::from_raw(self.0) };
        self.0 = std::ptr::null_mut();
        *b
    }
    pub fn get_ref(&self) -> &T {
        if self.0.is_null() {
            panic!("Ups.");
        }
        unsafe { self.0.as_ref().unwrap() }
    }
    pub fn get_mut(&mut self) -> &mut T {
        if self.0.is_null() {
            panic!("Ups.");
        }
        unsafe { self.0.as_mut().unwrap() }
    }
    pub fn is_valid(&self) -> bool {
        !self.0.is_null()
    }
}

pub trait AnyPromises {
    type Result: 'static;
    fn register(self) -> Promise<Self::Result, (), ()>;
}
pub trait AllPromises {
    type Result: 'static;
    fn register(self) -> Promise<Self::Result, (), ()>;
}

impl<R: 'static, E: 'static, S: 'static> AnyPromises for Vec<Promise<R, E, S>> {
    type Result = (S, Result<R, E>);
    fn register(self) -> Promise<Self::Result, (), ()> {
        let ids: Vec<PromiseId> = self.iter().map(|p| p.id).collect();
        let discard_ids = ids.clone();
        Promise::register(
            move |world, any_id| {
                let mut idx = 0usize;
                for promise in self {
                    let ids = ids.clone();
                    promise_register(
                        world,
                        promise.map(move |s| (s, any_id, idx, ids)).then(asyn!(|s, r| {
                            let (state, any_id, idx, ids) = s.value;
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    for (i, id) in ids.iter().enumerate() {
                                        if i != idx {
                                            promise_discard::<R, E, S>(world, *id);
                                        }
                                    }
                                    promise_resolve::<(S, Result<R, E>), (), ()>(world, any_id, (state, r), ())
                                },
                                |_, _| {},
                            )
                        })),
                    );
                    idx += 1;
                }
            },
            move |world, _| {
                for id in discard_ids {
                    promise_discard::<R, E, S>(world, id);
                }
            },
        )
    }
}

impl<R: 'static, E: 'static, S: 'static> AllPromises for Vec<Promise<R, E, S>> {
    type Result = Vec<Result<R, E>>;
    fn register(self) -> Promise<Self::Result, (), ()> {
        let ids: Vec<PromiseId> = self.iter().map(|p| p.id).collect();
        let size = ids.len();
        Promise::register(
            move |world, any_id| {
                let value: Vec<Option<Result<R, E>>> = (0..size).map(|_| None).collect();
                let value = MutPtr::new(value);
                let mut idx = 0usize;
                for promise in self {
                    promise_register(
                        world,
                        promise.with((any_id, idx, value.clone())).then(asyn!(|s, r| {
                            let (any_id, idx, mut value) = s.value;
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    value.get_mut()[idx] = Some(r);
                                    if value.get_ref().iter().all(|v| v.is_some()) {
                                        let value = value.get().into_iter().map(|v| v.unwrap()).collect();
                                        promise_resolve::<Vec<Result<R, E>>, (), ()>(world, any_id, value, ())
                                    }
                                },
                                |_, _| {},
                            )
                        })),
                    );
                    idx += 1;
                }
            },
            move |world, _| {
                for id in ids {
                    promise_discard::<R, E, S>(world, id);
                }
            },
        )
    }
}

impl_any_promises! { 8 }
impl_all_promises! { 8 }

pub struct Promises<R: 'static, E: 'static, S: 'static>(Vec<Promise<R, E, S>>);
impl<R: 'static, E: 'static, S: 'static> Promises<R, E, S> {
    pub fn any(self) -> Promise<(S, Result<R, E>), (), ()> {
        PromiseState::new(()).any(self.0)
    }
    pub fn all(self) -> Promise<Vec<Result<R, E>>, (), ()> {
        PromiseState::new(()).all(self.0)
    }
}

pub trait PromisesExtension<R: 'static, E: 'static, S: 'static> {
    fn promise(self) -> Promises<R, E, S>;
}

impl<R: 'static, E: 'static, S: 'static, I: Iterator<Item = Promise<R, E, S>>> PromisesExtension<R, E, S> for I {
    fn promise(self) -> Promises<R, E, S> {
        Promises(self.collect())
    }
}
