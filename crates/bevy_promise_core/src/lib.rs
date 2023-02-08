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
        (None, None) => { },
        _ => error!("Misconfigured promise")
    }
    registry.0.write().unwrap().remove(&id);
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
        let (r, rr) = (mem::take(&mut prom.reject), mem::take(&mut prom.resolve_reject));
        (r, rr)
    } {
        (Some(reject), None) => reject(world, error, state),
        (None, Some(resolve_reject)) => resolve_reject(world, Err(error), state),
        (None, None) => { },
        _ => error!("Misconfigured promise")
    }
    registry.0.write().unwrap().remove(&id);
}

pub fn promise_register<R: 'static, E: 'static, S: 'static>(
    world: &mut World,
    mut promise: Promise<R, E, S>,
) {
    let id = promise.id;
    let register = promise.register;
    promise.register = None;
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<R, E, S>::default)
        .clone();
    registry.0.write().unwrap().insert(id, promise);
    if let Some(register) = register {
        register(world, id)
    }
}

pub struct Promise<R, E, S> {
    id: PromiseId,
    register: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
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

impl<R: 'static, E: 'static, S: 'static> PromiseResult<R, E, S> {
    pub fn with<S2: 'static>(self, state: S2) -> PromiseResult<R, E, S2> {
        let state = OnceValue::new(state);
        match self {
            Self::Resolve(r, _) => PromiseResult::Resolve(r, state.get()),
            Self::Rejected(e, _) => PromiseResult::Rejected(e, state.get()),
            Self::Await(p) => PromiseResult::Await({
                p.then_catch(move |In((_, AsyncValue(r)))| {
                    match r {
                        Ok(r) => {
                            PromiseResult::Resolve(r, state.get())
                        }
                        Err(e) => {
                            PromiseResult::Rejected(e, state.get())
                        }
                    }
                })
            }),
        }
    }
    pub fn result<R2: 'static>(self, result: R2) -> PromiseResult<R2, E, S> {
        let result = OnceValue::new(result);
        match self {
            Self::Resolve(_, c) => PromiseResult::Resolve(result.get(), c),
            Self::Rejected(e, c) => PromiseResult::Rejected(e, c),
            Self::Await(p) => PromiseResult::Await(
                p.then(move |In((AsyncState(c), _))| PromiseResult::Resolve(result.get(), c.value)),
            ),
        }
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
            "PromiseId({}:{})",
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
    pub fn new<Params, F: 'static + IntoSystem<AsyncState<()>, PromiseResult<R, E, S>, Params>>(
        func: F,
    ) -> Promise<R, E, S> {
        let id = Self::id();
        Promise {
            id,
            resolve: None,
            reject: None,
            resolve_reject: None,
            register: Some(Box::new(move |world, id| {
                let mut system = IntoSystem::into_system(func);
                system.initialize(world);
                let pr = system.run(AsyncState(PromiseState::new(())), world);
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

    pub fn register<F: 'static + FnOnce(&mut World, PromiseId)>(func: F) -> Promise<R, E, S> {
        Promise {
            id: Self::id(),
            resolve: None,
            reject: None,
            resolve_reject: None,
            register: Some(Box::new(func)),
        }
    }

    pub fn then<
        Params,
        R2: 'static,
        E2: 'static,
        C2: 'static,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<R>), PromiseResult<R2, E2, C2>, Params>,
    >(
        mut self,
        func: F,
    ) -> Promise<R2, E2, C2> {
        let id = Self::id();
        self.resolve = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run(
                (AsyncState(PromiseState::new(state)), AsyncValue(result)),
                world,
            );
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
            resolve_reject: None,
            resolve: None,
            reject: None,
        }
    }

    pub fn then_catch<
        Params, R2: 'static, E2: 'static, C2: 'static,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<Result<R, E>>), PromiseResult<R2, E2, C2>, Params>
    >(
        mut self, func: F
    ) -> Promise<R2, E2, C2> {
        let id = Self::id();
        self.resolve_reject = Some(Box::new(move |world, result, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run((AsyncState(PromiseState::new(state)), AsyncValue(result)), world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => {
                    promise_resolve::<R2, E2, C2>(world, id, r, c)
                },
                PromiseResult::Rejected(e, c) => {
                    promise_reject::<R2, E2, C2>(world, id, e, c)
                }
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!("Misconfigured {}<{}, {}>, resolve/reject already defined", p.id, type_name::<R2>(), type_name::<E2>());
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
            resolve: None,
            reject: None,
            resolve_reject: None,
        }
    }
    
    pub fn catch<
        Params,
        F: 'static + IntoSystem<(AsyncState<S>, AsyncValue<E>), PromiseResult<R, (), S>, Params>,
    >(
        mut self,
        func: F,
    ) -> Promise<R, (), S> {
        let id = self.id;
        self.reject = Some(Box::new(move |world, error, state| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run(
                (AsyncState(PromiseState { value: state }), AsyncValue(error)),
                world,
            );
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => promise_resolve::<R, E, S>(world, id, r, c),
                PromiseResult::Rejected(_, _) => {
                    error!(
                        "Misconfigured {}<{}, {}>: catch exit with empty (dropped) error",
                        id,
                        type_name::<R>(),
                        type_name::<E>()
                    );
                }
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() || p.resolve_reject.is_some() || p.reject.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}>: resolve already defined",
                            id,
                            type_name::<R>(),
                            type_name::<E>()
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
        Self {
            id,
            result
        }
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
    pub struct Timer<T>(T);
    impl<T: 'static> Timer<T> {
        pub fn delay(self, duration: f32) -> PromiseResult<(), (), T> {
            PromiseResult::Await(Promise::<(), (), ()>::register(move |world, id| {
                let time = world.resource::<Time>();
                let end = time.elapsed_seconds() + duration - time.delta_seconds();
                world.resource_mut::<Timers>().push((id, end));
            }))
            .with(self.0)
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
    pub struct Timers(Vec<(PromiseId, f32)>);

    pub fn process_timers(time: Res<Time>, mut commands: Commands, mut timers: ResMut<Timers>) {
        let elapsed = time.elapsed_seconds();
        // timers.iter_mut().for_each(|(_, delay)| *delay -= delta);
        let mut new_timers = vec![];
        for (id, end) in timers.iter() {
            let id = *id;
            if &elapsed >= end {
                commands.add(PromiseCommand::ok(id, ()));
            } else {
                new_timers.push((id, *end));
            }
        }
        timers.0 = new_timers;
    }
}

pub struct PromiseState<S> {
    pub value: S
}
impl<S> PromiseState<S> {
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
    pub fn with<T, F: FnOnce(S) -> T>(self, map: F) -> PromiseState<T> {
        PromiseState{ value: map(self.value) }
    }
}

impl<T: std::fmt::Display> std::fmt::Display for PromiseState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PromiseState({})", self.value)
    }
}
