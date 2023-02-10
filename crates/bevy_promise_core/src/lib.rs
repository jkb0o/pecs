use std::{
    any::type_name,
    cell::RefCell,
    marker::PhantomData,
    mem,
    ops::Deref,
    sync::{Arc, RwLock},
    thread::{self, ThreadId},
};

use bevy::{
    ecs::system::{Command, SystemParam, SystemParamItem},
    prelude::*,
    utils::HashMap,
};

pub struct AsyncOps<T>(pub T);
pub struct AsyncState<T>(pub PromiseState<T>);
pub struct AsyncValue<T>(pub T);

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

pub struct PromiseFunction<Input, Output, Params: PromiseParams> {
    pub marker: PhantomData<Params>,
    pub body: fn(In<Input>, SystemParamItem<Params>) -> Output,
}
impl<Input, Output, Params: PromiseParams> PromiseFunction<Input, Output, Params> {
    fn new(body: fn(In<Input>, SystemParamItem<Params>) -> Output) -> Self {
        PromiseFunction {
            body,
            marker: PhantomData,
        }
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

    pub fn new<D: 'static, Params: PromiseParams, P: 'static + Into<PromiseResult<R, E, S>>>(
        default_state: D,
        func: PromiseFunction<PromiseState<D>, P, Params>,
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
                let mut system = IntoSystem::into_system(func.body);
                system.initialize(world);
                let pr = system.run(PromiseState::new(default_state), world).into();
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
                        p.resolve = Some(Box::new(move |world, r, c| promise_resolve::<R, E, S>(world, id, r, c)));
                        promise_register::<R, E, S>(world, p);
                    }
                }
            })),
        }
    }

    pub fn register<F: 'static + FnOnce(&mut World, PromiseId), D: 'static + FnOnce(&mut World, PromiseId)>(
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

    pub fn ok_then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        mut self,
        func: PromiseFunction<(PromiseState<S>, R), P, Params>,
    ) -> Promise<R2, E2, S2> {
        let id = Self::id();
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
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E2, S2>(world, id, e, c),
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
        Promise {
            id,
            register: Some(Box::new(move |world, _id| promise_register::<R, E, S>(world, self))),
            discard: None,
            resolve_reject: None,
            resolve: None,
            reject: None,
        }
    }

    pub fn then<
        R2: 'static,
        E2: 'static,
        S2: 'static,
        Params: PromiseParams,
        P: 'static + Into<PromiseResult<R2, E2, S2>>,
    >(
        mut self,
        func: PromiseFunction<(PromiseState<S>, Result<R, E>), P, Params>,
    ) -> Promise<R2, E2, S2> {
        let id = Self::id();
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
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E2, S2>(world, id, e, c),
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

    pub fn map<R2: 'static, F: 'static + FnOnce(R) -> R2>(mut self, map: F) -> Promise<R2, E, S> {
        let id = Self::id();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R2, E, S>(world, id);
        }));
        let map = OnceValue::new(map);
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
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E, S>(world, id, e, c),
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

    pub fn map_err<E2: 'static, F: 'static + FnOnce(E) -> E2>(mut self, map: F) -> Promise<R, E2, S> {
        let id = Self::id();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R, E2, S>(world, id);
        }));
        let map = OnceValue::new(map);
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
                PromiseResult::Rejected(e, c) => promise_reject::<R, E2, S>(world, id, e, c),
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

    pub fn map_state<S2: 'static, F: 'static + FnOnce(S) -> S2>(mut self, map: F) -> Promise<R, E, S2> {
        let id = Self::id();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<R, E, S2>(world, id);
        }));
        let map = OnceValue::new(map);
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(move |In((s, r)): In<(PromiseState<S>, Result<R, E>)>| {
                PromiseState::new(map.get()(s.value)).result(r)
            });
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), result), world).into();
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S2>(world, id, r, c),
                PromiseResult::Rejected(e, c) => promise_reject::<R, E, S2>(world, id, e, c),
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

    pub fn or_else<Params: PromiseParams, P: 'static + Into<PromiseResult<R, (), S>>>(
        mut self,
        func: PromiseFunction<(PromiseState<S>, E), P, Params>,
    ) -> Promise<R, (), S> {
        let id = self.id;
        self.reject = Some(Box::new(move |world, error, state| {
            let mut system = IntoSystem::into_system(func.body);
            system.initialize(world);
            let pr = system.run((PromiseState::new(state), error), world).into();
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
        self.ok_then(PromiseFunction::<_, _, ()>::new(|In((c, r)), ()| {
            PromiseResult::<R, (), S>::Resolve(r, c.value)
        }))
    }

    pub fn returns<R2: 'static>(self, value: R2) -> Promise<R2, E, S> {
        self.map(|_| value)
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
    pub trait TimerOpsExtension<T> {
        fn timeout(self, duration: f32) -> Promise<(), (), T>;
    }
    impl<T: 'static> TimerOpsExtension<T> for AsyncOps<T> {
        fn timeout(self, duration: f32) -> Promise<(), (), T> {
            timeout(duration).map_state(|_| self.0)
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
    pub fn cmd(self) -> AsyncOps<S> {
        AsyncOps(self.value)
    }
    pub fn asyn(self) -> AsyncOps<S> {
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
        PromiseState { value: map(self.value) }
    }

    pub fn then<R: 'static, E: 'static, S2: 'static>(self, promise: Promise<R, E, S2>) -> Promise<R, E, S> {
        promise.map_state(|_| self.value)
    }

    pub fn any<A: AnyPromise>(self, any: A) -> Promise<A::Result, (), S> {
        any.register().map_state(|_| self.value)
    }
    pub fn all<A: AllPromise>(self, all: A) -> Promise<A::Result, (), S> {
        all.register().map_state(|_| self.value)
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
    pub fn as_ref(&self) -> &T {
        if self.0.is_null() {
            panic!("Ups.");
        }
        unsafe { self.0.as_ref().unwrap() }
    }
    pub fn is_valid(&self) -> bool {
        !self.0.is_null()
    }
    pub fn map<F: FnOnce(&mut T)>(&mut self, map: F) {
        if self.0.is_null() {
            panic!("Ups.");
        }
        unsafe {
            map(self.0.as_mut().unwrap());
        }
        // let b = unsafe { Box::from_raw(self.0) };
        // let b = Box::new(map(*b));
        // self.0 = Box::leak(b) as *mut T
    }
}

pub trait AnyPromise {
    type Items;
    type Result: 'static;

    fn register(self) -> Promise<Self::Result, (), ()>;
}

impl<R0: 'static, R1: 'static, E0: 'static, E1: 'static> AnyPromise for (Promise<R0, E0, ()>, Promise<R1, E1, ()>) {
    type Items = (PromiseId, PromiseId);
    type Result = (Option<Result<R0, E0>>, Option<Result<R1, E1>>);
    fn register(self) -> Promise<Self::Result, (), ()> {
        let (p0, p1) = self;
        let (id0, id1) = (p0.id, p1.id);
        Promise::register(
            move |world, any_id| {
                promise_register(
                    world,
                    p0.map_state(move |_| (any_id, id1))
                        .then(PromiseFunction::<_, _, ()>::new(|In((s, r)), ()| {
                            let (any_id, id1) = s.value.clone();
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    // discard rest promises
                                    promise_discard::<R1, E1, ()>(world, id1);
                                    // resolve p0
                                    promise_resolve::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>), (), ()>(
                                        world,
                                        any_id,
                                        (Some(r), None),
                                        (),
                                    );
                                },
                                move |_world, _id| {},
                            )
                        })),
                );
                promise_register(
                    world,
                    p1.map_state(move |_| (any_id, id0))
                        .then(PromiseFunction::<_, _, ()>::new(|In((s, r)), ()| {
                            let (any_id, id0) = s.value.clone();
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    // discard rest promises
                                    promise_discard::<R0, E0, ()>(world, id0);
                                    // resolve p0
                                    promise_resolve::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>), (), ()>(
                                        world,
                                        any_id,
                                        (None, Some(r)),
                                        (),
                                    );
                                },
                                move |_world, _id| {},
                            )
                        })),
                );
            },
            move |world, _id| {
                promise_discard::<R0, E0, ()>(world, id0);
                promise_discard::<R1, E1, ()>(world, id1);
            },
        )
    }
}

pub trait AllPromise {
    type Items;
    type Result: 'static;

    fn register(self) -> Promise<Self::Result, (), ()>;
}

impl<R0: 'static, R1: 'static, E0: 'static, E1: 'static> AllPromise for (Promise<R0, E0, ()>, Promise<R1, E1, ()>) {
    type Items = (PromiseId, PromiseId);
    type Result = (Result<R0, E0>, Result<R1, E1>);
    fn register(self) -> Promise<Self::Result, (), ()> {
        let (p0, p1) = self;
        let (id0, id1) = (p0.id, p1.id);
        let value0 = MutPtr::<(Option<Result<R0, E0>>, Option<Result<R1, E1>>)>::new((None, None));
        let value1 = value0.clone();

        Promise::register(
            move |world, any_id| {
                promise_register(
                    world,
                    p0.map_state(move |_| (value0, any_id))
                        .then(PromiseFunction::<_, _, ()>::new(|In((s, r)), ()| {
                            let (mut value, any_id) = s.value.clone();
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    value.map(|(v0, _)| *v0 = Some(r));
                                    if value.is_valid() && value.as_ref().0.is_some() && value.as_ref().1.is_some() {
                                        let (v0, v1) = value.get();
                                        promise_resolve::<(Result<R0, E0>, Result<R1, E1>), (), ()>(
                                            world,
                                            any_id,
                                            (v0.unwrap(), v1.unwrap()),
                                            (),
                                        );
                                    }
                                    // resolve p0
                                },
                                move |_world, _id| {},
                            )
                        })),
                );
                promise_register(
                    world,
                    p1.map_state(move |_| (value1, any_id))
                        .then(PromiseFunction::<_, _, ()>::new(|In((s, r)), ()| {
                            let (mut value, any_id) = s.value.clone();
                            Promise::<(), (), ()>::register(
                                move |world, _id| {
                                    value.map(|(_, v1)| *v1 = Some(r));
                                    if value.is_valid() && value.as_ref().0.is_some() && value.as_ref().1.is_some() {
                                        let (v0, v1) = value.get();
                                        promise_resolve::<(Result<R0, E0>, Result<R1, E1>), (), ()>(
                                            world,
                                            any_id,
                                            (v0.unwrap(), v1.unwrap()),
                                            (),
                                        );
                                    }
                                    // resolve p0
                                },
                                move |_world, _id| {},
                            )
                        })),
                );
            },
            move |world, id| {
                promise_discard::<R0, E0, ()>(world, id0);
                promise_discard::<R1, E1, ()>(world, id1);
            },
        )
    }
}

#[derive(Resource)]
struct AnyPromiseResults2<R0, E0, R1, E1>(HashMap<PromiseId, (Option<Result<R0, E0>>, Option<Result<R1, E1>>)>);
unsafe impl<R0, E0, R1, E1> Send for AnyPromiseResults2<R0, E0, R1, E1> {}
unsafe impl<R0, E0, R1, E1> Sync for AnyPromiseResults2<R0, E0, R1, E1> {}

pub fn process_any_promise_2<R0, E0, R1, E1>() {}
