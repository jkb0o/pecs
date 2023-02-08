use std::{sync::{Arc, RwLock}, cell::RefCell, thread::{ThreadId, self}, any::type_name, mem};

use bevy::{prelude::*, utils::HashMap, ecs::system::Command};

pub mod prelude {
    pub use super::PromisePlugin;
    pub use super::Promise;
}

pub struct PromisePlugin;
impl Plugin for PromisePlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(timer::Timers::default())
            .add_system(timer::process_timers);
    }
}
pub struct AsyncOps<T>(T);

pub struct AsyncState<T>(pub PromiseState<T>);
pub struct AsyncValue<T>(pub T);

pub fn promise_resolve<R: 'static, E: 'static, C: 'static>(world: &mut World, id: PromiseId, result: R, context: C) {
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E, C>::default).clone();
    // let prom = registry.0.write().unwrap().remove(&id).unwrap();
    if let Some(resolve) = {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        mem::take(&mut prom.resolve)
    } {
        resolve(world, result, context);
    }
    registry.0.write().unwrap().remove(&id);
}

pub fn promise_reject<R: 'static, E: 'static, C: 'static>(world: &mut World, id: PromiseId, error: E, context: C) {
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E, C>::default).clone();
    if let Some(reject) = {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        mem::take(&mut prom.reject)
    } {
        reject(world, error, context);
    }
    registry.0.write().unwrap().remove(&id);
}

pub fn promise_register<R: 'static, E: 'static, C: 'static>(world: &mut World, mut promise: Promise<R, E, C>) {
    let id = promise.id;
    let register = promise.register;
    promise.register = None;
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E, C>::default).clone();
    registry.0.write().unwrap().insert(id, promise);
    if let Some(register) = register {
        register(world, id)
    }
}

pub struct Promise<R, E, C> {
    id: PromiseId,
    register: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    resolve: Option<Box<dyn FnOnce(&mut World, R, C)>>,
    reject: Option<Box<dyn FnOnce(&mut World, E, C)>>,
}
unsafe impl<R, E, C> Send for Promise<R, E, C> {}
unsafe impl<R, E, C> Sync for Promise<R, E, C> {}

pub enum PromiseResult<R, E, C> {
    Resolve(R, C),
    Rejected(E, C),
    Await(Promise<R, E, C>)
}

pub struct OnceValue<T>(*mut T);
unsafe impl<T> Send for OnceValue<T> {}
unsafe impl<T> Sync for OnceValue<T> {}
impl<T> OnceValue<T> {
    pub fn new(value: T) -> OnceValue<T>{
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

impl<R: 'static, E: 'static, C: 'static> PromiseResult<R, E, C> {
    pub fn context<C2: 'static>(self, context: C2) -> PromiseResult<R, E, C2> {
        let context = OnceValue::new(context);
        match self {
            Self::Resolve(r, _) => PromiseResult::Resolve(r, context.get()),
            Self::Rejected(e, _) => PromiseResult::Rejected(e, context.get()),
            Self::Await(p) => PromiseResult::Await(p.then(move |In((_, AsyncValue(r)))| {
                PromiseResult::Resolve(r, context.get())
            }))
        }
    }
    pub fn result<R2: 'static>(self, result: R2) -> PromiseResult<R2, E, C> {
        let result = OnceValue::new(result);
        match self {
            Self::Resolve(_, c) => PromiseResult::Resolve(result.get(), c),
            Self::Rejected(e, c) => PromiseResult::Rejected(e, c),
            Self::Await(p) => PromiseResult::Await(p.then(move |In((AsyncState(c), _))| {
                PromiseResult::Resolve(result.get(), c.0)
            }))
        }
    }
}

#[derive(Resource)]
struct PromiseRegistry<R, E, C>(Arc<RwLock<HashMap<PromiseId, Promise<R, E, C>>>>);
impl<R, E, C> Default for PromiseRegistry<R, E, C> {
    fn default() -> Self {
        PromiseRegistry(Arc::new(RwLock::new(HashMap::new())))
    }
}
impl<R, E, C> Clone for PromiseRegistry<R, E, C> {
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
impl <E: 'static> Promise<(), E, ()> {
    #[allow(non_snake_case)]
    pub fn Err(err: E) -> PromiseResult<(), E, ()> {
        PromiseResult::Rejected(err, ())
    }
}
impl <R: 'static> Promise<R, (), ()> {
    #[allow(non_snake_case)]
    pub fn Ok(result: R) -> PromiseResult<R, (), ()> {
        PromiseResult::Resolve(result, ())
    }
}
impl <R: 'static, E: 'static> Promise<R, E, ()> {
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
impl <R: 'static, E: 'static, C: 'static> Promise<R, E, C> {

    pub fn new<Params, F: 'static + IntoSystem<AsyncState<()>, PromiseResult<R, E, C>, Params>>(func: F) -> Promise<R, E, C> {
        let id = Self::id();
        Promise {
            id,
            resolve: None,
            reject: None,
            register: Some(Box::new(move |world, id| {
                let mut system = IntoSystem::into_system(func);
                system.initialize(world);
                let pr = system.run(AsyncState(PromiseState::new(())), world);
                system.apply_buffers(world);
                match pr {
                    PromiseResult::Resolve(r, c) => {
                        promise_resolve::<R, E, C>(world, id, r, c)
                    },
                    PromiseResult::Rejected(e, c) => promise_reject::<R, E, C>(world, id, e, c),
                    PromiseResult::Await(mut p) => {
                        if p.resolve.is_some() {
                            error!("Misconfigured {}<{}, {}, {}>, resolve already defined", p.id, type_name::<R>(), type_name::<E>(), type_name::<C>());
                            return;
                        }
                        p.resolve = Some(Box::new(move |world, r, c| {
                            promise_resolve::<R, E, C>(world, id, r, c)
                        }));
                        promise_register::<R, E, C>(world, p);
                    }
                }
            }))
        }
    }

    pub fn id() -> PromiseId {
        PROMISE_LOCAL_ID.with(|id| {
            let mut new_id = id.borrow_mut();
            *new_id += 1;
            PromiseId {
                thread: thread::current().id(),
                local: *new_id
            }
        })
    }

    pub fn register<F: 'static + FnOnce(&mut World, PromiseId)>(func: F) -> Promise<R, E, C> {
        Promise {
            id: Self::id(),
            resolve: None,
            reject: None,
            register: Some(Box::new(func))
        }
    }



    pub fn then<Params, R2: 'static, E2: 'static, C2: 'static, F: 'static + IntoSystem<(AsyncState<C>, AsyncValue<R>), PromiseResult<R2, E2, C2>, Params>>(mut self, func: F) -> Promise<R2, E2, C2> {
        let id = Self::id();
        self.resolve = Some(Box::new(move |world, result, context| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run((AsyncState(PromiseState::new(context)), AsyncValue(result)), world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => {
                    promise_resolve::<R2, E2, C2>(world, id, r, c)
                },
                PromiseResult::Rejected(e, c) => promise_reject::<R2, E2, C2>(world, id, e, c),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!("Misconfigured {}<{}, {}>, resolve already defined", p.id, type_name::<R2>(), type_name::<E2>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R2, E2, C2>(world, id, r, c);
                    }));
                    promise_register::<R2, E2, C2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<R, E, C>(world, self)
            })), 
            resolve: None,
            reject: None
        }
    }

    // pub fn then_catch<
    //     Params, R2: 'static, E2: 'static,
    //     FR: 'static + IntoSystem<R, PromiseResult<R2, E2>, Params>,
    //     FE: 'static + IntoSystem<E, PromiseResult<R2, E2>, Params>,
    // >
    // (mut self, resolve: FR, reject: FE) -> Promise<R2, E2> {
    //     let id = Self::id();
    //     self.resolve = Some(Box::new(move |world, result| {
    //         let mut system = IntoSystem::into_system(resolve);
    //         system.initialize(world);
    //         let pr = system.run(result, world);
    //         system.apply_buffers(world);
    //         match pr {
    //             PromiseResult::Resolve(r) => {
    //                 promise_resolve::<R2, E2>(world, id, r)
    //             },
    //             PromiseResult::Rejected(e) => promise_reject::<R2, E2>(world, id, e),
    //             PromiseResult::Await(mut p) => {
    //                 if p.resolve.is_some() {
    //                     error!("Misconfigured {}<{}, {}>, resolve already defined", p.id, type_name::<R2>(), type_name::<E2>());
    //                     return;
    //                 }
    //                 p.resolve = Some(Box::new(move |world, r| {
    //                     promise_resolve::<R2, E2>(world, id, r);
    //                 }));
    //                 promise_register::<R2, E2>(world, p);
    //             }
    //         }
    //     }));
    //     self.reject = Some(Box::new(move |world, error| {
    //         let mut system = IntoSystem::into_system(reject);
    //         system.initialize(world);
    //         let pr = system.run(error, world);
    //         system.apply_buffers(world);
    //         match pr {
    //             PromiseResult::Resolve(r) => {
    //                 promise_resolve::<R2, E2>(world, id, r)
    //             },
    //             PromiseResult::Rejected(e) => promise_reject::<R2, E2>(world, id, e),
    //             PromiseResult::Await(mut p) => {
    //                 if p.reject.is_some() {
    //                     error!("Misconfigured {}<{}, {}>, reject already defined", p.id, type_name::<R2>(), type_name::<E2>());
    //                     return;
    //                 }
    //                 p.resolve = Some(Box::new(move |world, r| {
    //                     promise_resolve::<R2, E2>(world, id, r);
    //                 }));
    //                 p.reject = Some(Box::new(move | world, e| {
    //                     promise_reject::<R2, E2>(world, id, e);
    //                 }));
    //                 promise_register::<R2, E2>(world, p);
    //             }
    //         }
    //     }));
    //     Promise {
    //         id,
    //         register: Some(Box::new(move |world, _id| {
    //             <World as WorldPromise<R, E>>::register_promise(world, self.id, self);
    //         })), 
    //         resolve: None,
    //         reject: None
    //     }
    // }

    pub fn catch<Params, F: 'static + IntoSystem<(AsyncState<C>, AsyncValue<E>), PromiseResult<R, (), C>, Params>>(mut self, func: F) -> Promise<R, (), C> {
        let id = self.id;
        self.reject = Some(Box::new(move |world, error, context| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run((AsyncState(PromiseState(context)), AsyncValue(error)), world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r, c) => {
                    promise_resolve::<R, E, C>(world, id, r, c)
                },
                PromiseResult::Rejected(_, _) => {
                    error!("Misconfigured {}<{}, {}>: catch exit with empty (dropped) error", id, type_name::<R>(), type_name::<E>());
                },
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!("Misconfigured {}<{}, {}>: resolve already defined", id, type_name::<R>(), type_name::<E>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r, c| {
                        promise_resolve::<R, E, C>(world, id, r, c);
                    }));
                }
            };
        }));
        self.then(move |In((AsyncState(c), AsyncValue(r)))| {
            PromiseResult::<R, (), C>::Resolve(r, c.0)
        })
    }
}

pub struct PromiseCommand<R, E> {
    id: PromiseId,
    result: Result<R, E>
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
}

impl<R: 'static + Send + Sync, E: 'static + Send + Sync> Command for PromiseCommand<R, E> {
    fn write(self, world: &mut World) {
        match self.result {
            Ok(r) => promise_resolve::<R, E, ()>(world, self.id, r, ()),
            Err(e) => promise_reject::<R, E, ()>(world, self.id, e, ()),
        }
    }
}

impl<R: 'static, E: 'static, C: 'static> Command for Promise<R, E, C> {
    fn write(self, world: &mut World) {
        promise_register::<R, E, C>(world, self)
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
            })).context(self.0)
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

    pub fn process_timers(
        time: Res<Time>,
        mut commands: Commands,
        mut timers: ResMut<Timers>,

    ) {
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

pub struct PromiseState<C>(pub C);
impl<C> PromiseState<C> {
    pub fn new(value: C) -> PromiseState<C> {
        PromiseState(value)
    }
    pub fn cmd(self) -> AsyncOps<C> {
        AsyncOps(self.0)
    }
    pub fn ops(self) -> AsyncOps<C> {
        AsyncOps(self.0)
    }
    pub fn resolve<R, E>(self, result: R) -> PromiseResult<R, E, C> {
        PromiseResult::Resolve(result, self.0)
    }
    pub fn ok<R>(self, result: R) -> PromiseResult<R, (), C> {
        PromiseResult::Resolve(result, self.0)
    }
    pub fn reject<R, E>(self, error: E) -> PromiseResult<R, E, C> {
        PromiseResult::Rejected(error, self.0)
    }
    pub fn get(&self) -> &C {
        &self.0
    }
    pub fn get_mut(&mut self) -> &mut C {
        &mut self.0
    }
    pub fn with<T, F: FnOnce(C) -> T>(self, map: F) -> PromiseState<T> {
        PromiseState(map(self.0))
    }
}

impl<T: std::fmt::Display> std::fmt::Display for PromiseState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PromiseContext({})", self.0)
    }
}