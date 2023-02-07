use std::{sync::{Arc, RwLock}, cell::RefCell, thread::{ThreadId, self}, any::type_name, mem};

use bevy::{prelude::*, utils::HashMap, ecs::system::Command};

pub mod prelude {
    pub use super::PromisePlugin;
    pub use super::Promise;
    pub use super::Async;
    pub use super::timer::TimerAsyncExtension;
}

pub struct PromisePlugin;
impl Plugin for PromisePlugin {
    fn build(&self, app: &mut App) {
        app
            .insert_resource(timer::Timers::default())
            .add_system(timer::process_timers);
    }
}

pub struct Async;

pub fn promise_resolve<R: 'static, E: 'static>(world: &mut World, id: PromiseId, result: R) {
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E>::default).clone();
    // let prom = registry.0.write().unwrap().remove(&id).unwrap();
    if let Some(resolve) = {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        mem::take(&mut prom.resolve)
    } {
        resolve(world, result);
    }
    registry.0.write().unwrap().remove(&id);
}

pub fn promise_reject<R: 'static, E: 'static>(world: &mut World, id: PromiseId, error: E) {
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E>::default).clone();
    if let Some(reject) = {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        mem::take(&mut prom.reject)
    } {
        reject(world, error);
    }
    registry.0.write().unwrap().remove(&id);
}

pub fn promise_register<R: 'static, E: 'static>(world: &mut World, mut promise: Promise<R, E>) {
    let id = promise.id;
    let register = promise.register;
    promise.register = None;
    let registry = world.get_resource_or_insert_with(PromiseRegistry::<R, E>::default).clone();
    registry.0.write().unwrap().insert(id, promise);
    if let Some(register) = register {
        register(world, id)
    }
}

pub trait WorldPromise<R,E> {
    fn resolve_promise(&mut self, id: PromiseId, result: R);
    fn reject_promise(&mut self, id: PromiseId, error: E);
    fn register_promise(&mut self, id: PromiseId, promise: Promise<R, E>);
}

impl <R: 'static, E: 'static> WorldPromise<R, E> for World {
    fn resolve_promise(&mut self, id: PromiseId, result: R) {
        let registry = self.get_resource_or_insert_with(PromiseRegistry::<R, E>::default).clone();
        let prom = registry.0.write().unwrap().remove(&id).unwrap();
        if let Some(resolve) = prom.resolve {
            resolve(self, result);
        }
        
    }
    fn reject_promise(&mut self, id: PromiseId, error: E) {
        
    }
    fn register_promise(&mut self, id: PromiseId, mut promise: Promise<R, E>) {
        let register = promise.register;
        promise.register = None;
        let registry = self.get_resource_or_insert_with(PromiseRegistry::<R, E>::default).clone();
        registry.0.write().unwrap().insert(id, promise);
        if let Some(register) = register {
            register(self, id)
        }
    }
    
}

pub struct Promise<R, E> {
    id: PromiseId,
    register: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    resolve: Option<Box<dyn FnOnce(&mut World, R)>>,
    reject: Option<Box<dyn FnOnce(&mut World, E)>>,
}
unsafe impl<R, E> Send for Promise<R, E> {}
unsafe impl<R, E> Sync for Promise<R, E> {}

pub enum PromiseResult<R, E> {
    Resolve(R),
    Rejected(E),
    Await(Promise<R, E>)
}

impl<R: 'static, E: 'static> PromiseResult<R, E> {
    pub fn context<C: 'static + Clone + Send + Sync>(self, context: C) -> PromiseResult<(R, C), E> {
        match self {
            Self::Resolve(r) => PromiseResult::Resolve((r, context)),
            Self::Rejected(e) => PromiseResult::Rejected(e),
            Self::Await(p) => PromiseResult::Await(p.then(move |In(r)| {
                PromiseResult::Resolve((r, context.clone()))
            }))
        }
    }
    pub fn result<R2: 'static + Clone + Send + Sync>(self, result: R2) -> PromiseResult<R2, E> {
        match self {
            Self::Resolve(_) => PromiseResult::Resolve(result),
            Self::Rejected(e) => PromiseResult::Rejected(e),
            Self::Await(p) => PromiseResult::Await(p.then(move |In(_)| {
                PromiseResult::Resolve(result.clone())
            }))
        }
    }

    // pub fn error<E2: 'static + Clone + Send + Sync>(self, error: E2) -> PromiseResult<R, E2> {
    //     match self {
    //         Self::Resolve(r) => PromiseResult::Resolve(r),
    //         Self::Rejected(_) => PromiseResult::Rejected(error),
    //         Self::Await(p) => PromiseResult::Await(p.catch(move |In(_)| {
    //             PromiseResult::Resolve(result.clone())
    //         }))
    //     }

    // }
}

#[derive(Resource)]
struct PromiseRegistry<R, E>(Arc<RwLock<HashMap<PromiseId, Promise<R, E>>>>);
impl<R, E> Default for PromiseRegistry<R, E> {
    fn default() -> Self {
        PromiseRegistry(Arc::new(RwLock::new(HashMap::new())))
    }
}
impl<R, E> Clone for PromiseRegistry<R, E> {
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
impl <E: 'static> Promise<(), E> {
    #[allow(non_snake_case)]
    pub fn Err(err: E) -> PromiseResult<(), E> {
        PromiseResult::Rejected(err)
    }
}
impl <R: 'static> Promise<R, ()> {
    #[allow(non_snake_case)]
    pub fn Ok(result: R) -> PromiseResult<R, ()> {
        PromiseResult::Resolve(result)
    }
}
impl <R: 'static, E: 'static> Promise<R, E> {
    #[allow(non_snake_case)]
    pub fn Resolve(result: R) -> PromiseResult<R, E> {
        PromiseResult::Resolve(result)
    }
    #[allow(non_snake_case)]
    pub fn Reject(error: E) -> PromiseResult<R, E> {
        PromiseResult::Rejected(error)
    }
    #[allow(non_snake_case)]
    pub fn Await(promise: Promise<R, E>) -> PromiseResult<R, E> {
        PromiseResult::Await(promise)
    }

    pub fn new<Params, F: 'static + IntoSystem<(), PromiseResult<R, E>, Params>>(func: F) -> Promise<R, E> {
        let id = Self::id();
        Promise {
            id,
            resolve: None,
            reject: None,
            register: Some(Box::new(move |world, id| {
                let mut system = IntoSystem::into_system(func);
                system.initialize(world);
                let pr = system.run((), world);
                system.apply_buffers(world);
                match pr {
                    PromiseResult::Resolve(r) => {
                        promise_resolve::<R, E>(world, id, r)
                    },
                    PromiseResult::Rejected(e) => promise_reject::<R, E>(world, id, e),
                    PromiseResult::Await(mut p) => {
                        if p.resolve.is_some() {
                            error!("Misconfigured {}<{}, {}>, resolve already defined", p.id, type_name::<R>(), type_name::<E>());
                            return;
                        }
                        p.resolve = Some(Box::new(move |world, r| {
                            promise_resolve::<R, E>(world, id, r)
                        }));
                        promise_register::<R, E>(world, p);
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

    pub fn register<F: 'static + FnOnce(&mut World, PromiseId)>(func: F) -> Promise<R, E> {
        Promise {
            id: Self::id(),
            resolve: None,
            reject: None,
            register: Some(Box::new(func))
        }
    }



    pub fn then<Params, R2: 'static, E2: 'static, F: 'static + IntoSystem<R, PromiseResult<R2, E2>, Params>>(mut self, func: F) -> Promise<R2, E2> {
        let id = Self::id();
        self.resolve = Some(Box::new(move |world, result| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run(result, world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r) => {
                    promise_resolve::<R2, E2>(world, id, r)
                },
                PromiseResult::Rejected(e) => promise_reject::<R2, E2>(world, id, e),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!("Misconfigured {}<{}, {}>, resolve already defined", p.id, type_name::<R2>(), type_name::<E2>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r| {
                        promise_resolve::<R2, E2>(world, id, r);
                    }));
                    promise_register::<R2, E2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                <World as WorldPromise<R, E>>::register_promise(world, self.id, self);
            })), 
            resolve: None,
            reject: None
        }
    }

    pub fn then_catch<
        Params, R2: 'static, E2: 'static,
        FR: 'static + IntoSystem<R, PromiseResult<R2, E2>, Params>,
        FE: 'static + IntoSystem<E, PromiseResult<R2, E2>, Params>,
    >
    (mut self, resolve: FR, reject: FE) -> Promise<R2, E2> {
        let id = Self::id();
        self.resolve = Some(Box::new(move |world, result| {
            let mut system = IntoSystem::into_system(resolve);
            system.initialize(world);
            let pr = system.run(result, world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r) => {
                    promise_resolve::<R2, E2>(world, id, r)
                },
                PromiseResult::Rejected(e) => promise_reject::<R2, E2>(world, id, e),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!("Misconfigured {}<{}, {}>, resolve already defined", p.id, type_name::<R2>(), type_name::<E2>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r| {
                        promise_resolve::<R2, E2>(world, id, r);
                    }));
                    promise_register::<R2, E2>(world, p);
                }
            }
        }));
        self.reject = Some(Box::new(move |world, error| {
            let mut system = IntoSystem::into_system(reject);
            system.initialize(world);
            let pr = system.run(error, world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r) => {
                    promise_resolve::<R2, E2>(world, id, r)
                },
                PromiseResult::Rejected(e) => promise_reject::<R2, E2>(world, id, e),
                PromiseResult::Await(mut p) => {
                    if p.reject.is_some() {
                        error!("Misconfigured {}<{}, {}>, reject already defined", p.id, type_name::<R2>(), type_name::<E2>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r| {
                        promise_resolve::<R2, E2>(world, id, r);
                    }));
                    p.reject = Some(Box::new(move | world, e| {
                        promise_reject::<R2, E2>(world, id, e);
                    }));
                    promise_register::<R2, E2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                <World as WorldPromise<R, E>>::register_promise(world, self.id, self);
            })), 
            resolve: None,
            reject: None
        }
    }

    pub fn catch<Params, F: 'static + IntoSystem<E, PromiseResult<R, ()>, Params>>(mut self, func: F) -> Promise<R, ()> {
        let id = self.id;
        self.reject = Some(Box::new(move |world, error| {
            let mut system = IntoSystem::into_system(func);
            system.initialize(world);
            let pr = system.run(error, world);
            system.apply_buffers(world);
            match pr {
                PromiseResult::Resolve(r) => {
                    promise_resolve::<R, E>(world, id, r)
                },
                PromiseResult::Rejected(_) => {
                    error!("Misconfigured {}<{}, {}>: catch exit with empty (dropped) error", id, type_name::<R>(), type_name::<E>());
                },
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!("Misconfigured {}<{}, {}>: resolve already defined", id, type_name::<R>(), type_name::<E>());
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, r| {
                        promise_resolve::<R, E>(world, id, r);
                    }));
                }
            };
        }));
        self.then(move |In(r)| {
            Promise::Ok(r)
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
            Ok(r) => <World as WorldPromise<R, E>>::resolve_promise(world, self.id, r),
            Err(e) => <World as WorldPromise<R, E>>::reject_promise(world, self.id, e),
        }
    }
}

impl<R: 'static, E: 'static> Command for Promise<R, E> {
    fn write(self, world: &mut World) {
        let id = self.id;
        <World as WorldPromise<R, E>>::register_promise(world, id, self);
    }
}

pub mod timer {
    use super::*;
    pub struct Timer;
    impl Timer {
        pub fn delay(&self, duration: f32) -> PromiseResult<(), ()> {
            PromiseResult::Await(Promise::register(move |world, id| {
                let time = world.resource::<Time>();
                let end = time.elapsed_seconds() + duration - time.delta_seconds();
                world.resource_mut::<Timers>().push((id, end));
            }))
        }
    }
    pub trait TimerAsyncExtension {
        fn timer() -> &'static Timer {
            &Timer
        }
    }
    impl TimerAsyncExtension for Async {}


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