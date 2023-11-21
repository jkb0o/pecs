//! Core [`Promise`] functionality.
use bevy::{
    ecs::system::{BoxedSystem, Command, StaticSystemParam, SystemParam},
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
pub mod app;
mod impls;
pub mod timer;
pub mod ui;

/// Namespace-like stateful container for asyn operations used to simplify
/// state passing through promise chain. For extending this container with
/// user-provided methods custom extension should be implemented:
/// ```ignore
/// fn my_async_func() -> Promise<(), ()> {
///     Promise::from(())
/// }
///
/// pub struct MyStatefulOps<S>(S);
/// impl<S: 'static> MyStatefulOps<S> {
///     pub fn func(self) -> Promise<S, ()> {
///         my_async_func().with(self.0)
///     }
/// }
///
/// pub trait MyAsyncExtension<S> {
///     fn my_async(self) -> MyStatefulOps<S>;
/// }
///
/// impl<S: 'static> MyAsyncExtension<S> for AsynOps<S> {
///     fn my_async(self) -> MyStatefulOps<S> {
///         MyStatefulOps(self.0)
///     }
/// }
///
/// // now you my_async_func could be used in both stateful/stateles ways
/// fn setup(mut commands: Commands) {
///     commands.add(
///         Promise::from(0)
///         .then(asyn!(state => {
///             // stateful, state passes to the next call
///             state.asyn().my_async().func()
///         }))
///         .then(asyn!(state => {
///             // stateless, state dropped
///             my_asyn_func()
///         }))
///     );
/// }
pub struct AsynOps<T>(pub T);
impl<T: Clone> Clone for AsynOps<T> {
    fn clone(&self) -> Self {
        AsynOps(self.0.clone())
    }
}
impl<T: Copy> Copy for AsynOps<T> {}

pub fn promise_resolve<S: 'static, R: 'static>(world: &mut World, id: PromiseId, state: S, result: R) {
    // info!(
    //     "resolving {id}<{}, {}>",
    //     type_name::<S>(),
    //     type_name::<R>(),
    // );
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<S, R>::default)
        .clone();
    if let Some(resolve) = {
        let mut write = registry.0.write().unwrap();
        let prom = write.get_mut(&id).unwrap();
        mem::take(&mut prom.resolve)
    } {
        resolve(world, state, result)
    }
    registry.0.write().unwrap().remove(&id);
    // info!(
    //     "resolved {id}<{}, {}> ({} left)",
    //     type_name::<S>(),
    //     type_name::<R>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub fn promise_register<S: 'static, R: 'static>(world: &mut World, mut promise: Promise<S, R>) {
    let id = promise.id;
    // info!("registering {id}");
    let register = promise.register;
    promise.register = None;
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<S, R>::default)
        .clone();
    registry.0.write().unwrap().insert(id, promise);
    if let Some(register) = register {
        register(world, id)
    }
    // info!(
    //     "registered {id}<{}, {}> ({} left)",
    //     type_name::<S>(),
    //     type_name::<R>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub fn promise_discard<S: 'static, R: 'static>(world: &mut World, id: PromiseId) {
    // info!("discarding {id}");
    let registry = world
        .get_resource_or_insert_with(PromiseRegistry::<S, R>::default)
        .clone();
    if let Some(discard) = {
        let mut write = registry.0.write().unwrap();
        if let Some(prom) = write.get_mut(&id) {
            mem::take(&mut prom.discard)
        } else {
            error!(
                "Internal promise error: trying to discard complete {id}<{}, {}>",
                type_name::<S>(),
                type_name::<R>(),
            );
            None
        }
    } {
        discard(world, id);
    }
    registry.0.write().unwrap().remove(&id);
    // info!(
    //     "discarded {id}<{}, {}> ({} left)",
    //     type_name::<S>(),
    //     type_name::<R>(),
    //     registry.0.read().unwrap().len()
    // );
}

pub trait PromiseParams: 'static + SystemParam + Send + Sync {}
impl<T: 'static + SystemParam + Send + Sync> PromiseParams for T {}

/// A wrapper around a system-like function that can be used in various contexts within `pecs`.
///
/// An `Asyn` function is constructed with the [`asyn!`] macro, and it can be passed to constructors such as
/// [`Promise::new`][Promise::new], [`Promise::from`][Promise::from], [`Promise::start`][Promise::start], and
/// [`Promise::repeat`][Promise::repeat], as well as to the chaining methods [`then`][PromiseLikeBase::then]
/// and [`then_repeat`][PromiseLike::then_repeat].
///
/// The signature of an `Asyn` function can be generated with the `Asyn!` macro. You can specify the input and output
/// state and result types, e.g., `Asyn![S, R => S2, R2]`. The function should return something that implements the
/// [`Into<PromiseResult<S2, R2>>`][PromiseResult] trait. By default, this is implemented for [`Promise<S, R>`],
/// [`PromiseState<S, ()>`], and [`()`].
///
/// The `Asyn` function can take optional parameters of type [`PromiseParams`] which allow the function to access the
/// same parameters as Bevy systems. These parameters are passed automatically by `pecs`.
pub struct Asyn<Input, Output: 'static, Params: PromiseParams> {
    pub marker: PhantomData<Params>,
    pub body: fn(In<Input>, StaticSystemParam<Params>) -> Output,
}
impl<Input, Otput: 'static, Params: PromiseParams> Clone for Asyn<Input, Otput, Params> {
    fn clone(&self) -> Self {
        Asyn {
            body: self.body.clone(),
            marker: self.marker,
        }
    }
}
impl<Input, Output: 'static, Params: PromiseParams> PartialEq for Asyn<Input, Output, Params> {
    fn eq(&self, other: &Self) -> bool {
        self.ptr() == other.ptr()
    }
}
impl<Input, Output: 'static, Params: PromiseParams> Eq for Asyn<Input, Output, Params> {}
impl<Input, Output: 'static, Params: PromiseParams> std::hash::Hash for Asyn<Input, Output, Params> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.ptr().hash(state)
    }
}
impl<Input, Output: 'static, Params: PromiseParams> Asyn<Input, Output, Params> {
    /// Creates a new `Asyn` from a system-like function pointer `body`.
    ///
    /// The `body` function takes two arguments: the input state of type `Input`,
    /// and a [`StaticSystemParam`] of type `Params`. The `Params` type can be used
    /// to access [system parameters][bevy::ecs::system::SystemParam] from within
    /// the `Asyn` function.
    ///
    /// The return value of the `body` function should be of type `Output`, which
    /// is then automatically converted into a [`PromiseResult`] using the
    /// [`Into<PromiseResult<S2, R2>>`] trait. The output state and result types
    /// for the resulting `Asyn` function are inferred from the return type of
    /// the `body` function.
    pub fn new(body: fn(In<Input>, StaticSystemParam<Params>) -> Output) -> Self {
        Asyn {
            body,
            marker: PhantomData,
        }
    }
    fn ptr(&self) -> *const fn(In<Input>, StaticSystemParam<Params>) -> Output {
        self.body as *const fn(In<Input>, StaticSystemParam<Params>) -> Output
    }
}
impl<Input: 'static, Output: 'static, Params: PromiseParams> Asyn<Input, Output, Params> {
    /// Executes the `Asyn` with the given `input` and [`World`][bevy::prelude::World].
    ///
    /// This method runs the `Asyn` with the given `input` and `World` context, returning
    /// the result of the execution. The `input` argument is used as the first argument
    /// when calling the system-like function `body` associated with the `Asyn`. The `World`
    /// argument is used to provide access to any necessary `SystemParam`s. The return
    /// value of the `run` method is the output of the system-like function.
    pub fn run(&self, input: Input, world: &mut World) -> Output {
        let registry = world
            .get_resource_or_insert_with(SystemRegistry::<Input, Output, Params>::default)
            .clone();
        let mut write = registry.0.write().unwrap();
        let key = self.clone();
        let system = write.entry(key).or_insert_with(|| {
            let mut sys = Box::new(IntoSystem::into_system(self.body));
            sys.initialize(world);
            sys
        });
        let result = system.run(input, world);
        system.apply_deferred(world);
        result
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

/// `PromiseResult` is the result of a promise, which can either resolve to a value with `S`
/// state and `R` result, or it can await another `Promise<S, R>`.
///
/// `S` stands for "state", which is the type of the data being carried through the promise chain.
/// `R` stands for "result", which is the type of the final value that the promise chain resolves to.
///
/// Promises returned by `asyn` functions should return a type that implements `Into<PromiseResult>`.
/// This allows them to interact with other promises in the chain.
pub enum PromiseResult<S, R> {
    Resolve(S, R),
    Await(Promise<S, R>),
}

impl<S, R> From<Promise<S, R>> for PromiseResult<S, R> {
    fn from(value: Promise<S, R>) -> Self {
        PromiseResult::Await(value)
    }
}
impl From<()> for PromiseResult<(), ()> {
    fn from(_: ()) -> Self {
        PromiseResult::Resolve((), ())
    }
}
impl<S: 'static> From<PromiseState<S>> for PromiseResult<S, ()> {
    fn from(state: PromiseState<S>) -> Self {
        PromiseResult::Resolve(state.value, ())
    }
}

#[derive(Resource)]
struct PromiseRegistry<S, R>(Arc<RwLock<HashMap<PromiseId, Promise<S, R>>>>);
impl<S, R> Default for PromiseRegistry<S, R> {
    fn default() -> Self {
        PromiseRegistry(Arc::new(RwLock::new(HashMap::new())))
    }
}
impl<S, R> Clone for PromiseRegistry<S, R> {
    fn clone(&self) -> Self {
        PromiseRegistry(self.0.clone())
    }
}

#[derive(Resource)]
struct SystemRegistry<In, Out: 'static, Params: PromiseParams>(
    Arc<RwLock<HashMap<Asyn<In, Out, Params>, BoxedSystem<In, Out>>>>,
);
impl<In, Out, Params: PromiseParams> Clone for SystemRegistry<In, Out, Params> {
    fn clone(&self) -> Self {
        SystemRegistry(self.0.clone())
    }
}
impl<In, Out, Params: PromiseParams> Default for SystemRegistry<In, Out, Params> {
    fn default() -> Self {
        SystemRegistry(Arc::new(RwLock::new(HashMap::new())))
    }
}

/// An enumeration used to control the behavior of a loop in a [`repeat(asyn!(...))`][Promise::repeat] construct.
///
/// A loop constructed with [`Promise::repeat()`] can be continued or broken by reolving promise with either
/// `Repeat::Continue` or `Repeat::Break(result)` from the underlying [`Asyn`][struct@Asyn] function. When
/// `Repeat::Continue` is returned, the loop repeats. When `Repeat::Break(result)` is returned, the loop breaks
/// and resolves with the given `result`.
pub enum Repeat<R> {
    /// A variant indicating that the loop should continue.
    Continue,
    /// A variant indicating that the loop should break with the given result.
    Break(R),
}

impl Repeat<()> {
    /// Creates an infinite repeat loop.
    pub fn forever() -> Self {
        Repeat::Continue
    }
}

/// A promise represents a value that may not be available yet, but will be in the future.
///
/// The promise's state is of type `S`, and the result type is `R`. The state represents the
/// current state of the promise which is carried through the promise chain, while the result
/// represents the final value that the promise will resolve to.
///
/// You can create new promises via [`Promise::start()`]
/// [`Promise::new()`], [`Promise::repeat()`] or
/// [`Promise::register(on_invoke, on_discard)`][Promise::register].
///
/// You can chain promises using the [`then()`][Promise::then] method, which takes
/// an [`Asyn`][struct@Asyn] function as an argument. This function takes the current promise state
/// and result as arguments, and may also take other [system parameters][bevy::ecs::system::SystemParam]
/// if needed.
///
/// The result of the `Asyn` function passes to the next promise in the chain immediately if
/// the result is [`Resolve`][PromiseResult::Resolve], or when a nested promise
/// is resolved if the result is [`Await`][PromiseResult::Await]. The type of
/// the next promise state/result arguments are inferred from the result of the previous promise.
pub struct Promise<S, R> {
    id: PromiseId,
    register: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    discard: Option<Box<dyn FnOnce(&mut World, PromiseId)>>,
    resolve: Option<Box<dyn FnOnce(&mut World, S, R)>>,
}
unsafe impl<S, R> Send for Promise<S, R> {}
unsafe impl<S, R> Sync for Promise<S, R> {}

impl<S: 'static> Promise<S, ()> {
    /// Creates a new `Promise` with the given initial state `state`.
    ///
    /// The resulting promise resolves immediately with the same state value,
    /// since the `Asyn` function returned by the `asyn!` macro simply passes
    /// the state through unchanged.
    /// ```ignore
    /// fn setup(mut commands: Commands) {
    ///     commands.add(
    ///         Promise::from(0)
    ///             .then(asyn!(state => {
    ///                 state.value += 1;
    ///                 state
    ///             }))
    ///             .then(asyn!(state => {
    ///                 state.value += 1;
    ///                 state
    ///             }))
    ///             .then(asyn!(state => {
    ///                 state.value += 1;
    ///                 info!("There was {} calls in the chain", state.value);
    ///             })),
    ///     );
    /// }
    /// ```
    pub fn from(state: S) -> Promise<S, ()> {
        Self::new(state, asyn!(s => s))
    }
}

impl<S: 'static, R: 'static> Promise<S, R> {
    /// Create new [`Promise`] with empty [state][PromiseState]
    /// ```ignore
    /// # use bevy::prelude::*
    /// fn setup(mut commands: Commands) {
    ///     commands.add(
    ///         // type of the `state` is infered as `PromiseState<()>`
    ///         Promise::start(asyn!(state => {
    ///             info!("I'm new Promise with empty state");
    ///             state.pass()
    ///         }))
    ///     );
    /// }
    /// ```
    pub fn start(func: Asyn![() => S, R]) -> Promise<S, R> {
        Promise::new((), func)
    }
    /// Create new [`Promise<S, R>`] from default `D` state and
    /// [`Asyn![D => S, R]`][struct@Asyn] func. `S` and `R` infers from the
    /// [`Asyn`][struct@Asyn] function body.
    /// ```ignore
    /// # use bevy::prelude::*
    /// fn setup(mut commands: Commands) {
    ///     let entity = commands.spawn_empty().id();
    ///     commands.add(
    ///         // type of the `state` is infered as `PromiseState<Entity>`
    ///         Promise::new(entity, asyn!(state => {
    ///             info!("I'm started with some entity {:?}", state.value);
    ///             state.pass()
    ///         }))
    ///     );
    /// }
    /// ```
    pub fn new<D: 'static>(default_state: D, func: Asyn![D => S, R]) -> Promise<S, R> {
        let id = PromiseId::new();
        Promise {
            id,
            resolve: None,
            discard: None,
            register: Some(Box::new(move |world, id| {
                // let mut system = world.promise_system(func);
                // let mut system = IntoSystem::into_system(func.body);
                // system.initialize(world);
                // let pr = system.run(PromiseState::new(default_state), world).into();
                // system.apply_buffers(world);
                // let pr = world.run_promise_system(func, PromiseState::new(default_state)).into();
                let pr = func.run((PromiseState::new(default_state), ()), world).into();
                match pr {
                    PromiseResult::Resolve(s, r) => promise_resolve::<S, R>(world, id, s, r),
                    PromiseResult::Await(mut p) => {
                        if p.resolve.is_some() {
                            error!(
                                "Misconfigured {}<{}, {}>, resolve already defined",
                                p.id,
                                type_name::<S>(),
                                type_name::<R>(),
                            );
                            return;
                        }
                        p.resolve = Some(Box::new(move |world, s, r| promise_resolve::<S, R>(world, id, s, r)));
                        promise_register::<S, R>(world, p);
                    }
                }
            })),
        }
    }

    /// Create new [`Promise`] with resolve/reject behaviour controlled by user.
    /// It takes two closures as arguments: `on_invoke` and `on_discard`. The
    /// `invoke` will be executed when the promise's turn comes. The discard
    /// will be called in the case of promise cancellation. Both closures takes
    /// [`&mut World`][bevy::prelude::World] and [`PromiseId`] as argiments.
    /// ```ignore
    /// #[derive(Component)]
    /// /// Holds PromiseId and the time when the timer should time out.
    /// pub struct MyTimer(PromiseId, f32);
    ///
    /// /// creates promise that will resolve after [`duration`] seconds
    /// pub fn delay(duration: f32) -> Promise<(), ()> {
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
    ///                 timers
    ///                     .iter(world)
    ///                     .filter(|(_entity, timer)| timer.0 == id)
    ///                     .map(|(entity, _timer)| entity)
    ///                     .next()
    ///             };
    ///             if let Some(entity) = entity {
    ///                 world.despawn(entity);
    ///             }
    ///         },
    ///     )
    /// }
    ///
    /// /// iterate ofver all timers and resolves completed
    /// pub fn process_timers_system(timers: Query<(Entity, &MyTimer)>, mut commands: Commands, time: Res<Time>) {
    ///     let now = time.elapsed_seconds();
    ///     for (entity, timer) in timers.iter().filter(|(_, t)| t.1 < now) {
    ///         let promise_id = timer.0;
    ///         commands.promise(promise_id).resolve(());
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
    ///         }))
    ///         .then(asyn!(s, _ => {
    ///             info!("Completing");
    ///             s.pass()
    ///         })),
    ///     );
    ///
    ///     // or queued directly to Commands
    ///     commands.add(delay(2.).then(asyn!(s, _ => {
    ///         info!("I'm another timer");
    ///         s.pass()
    ///     })));
    /// }
    /// ```
    pub fn register<F: 'static + FnOnce(&mut World, PromiseId), D: 'static + FnOnce(&mut World, PromiseId)>(
        on_invoke: F,
        on_discard: D,
    ) -> Promise<S, R> {
        Promise {
            id: PromiseId::new(),
            resolve: None,
            register: Some(Box::new(on_invoke)),
            discard: Some(Box::new(on_discard)),
        }
    }

    /// Create new [`Promise<S, R>`] from default `S` state and  [`Asyn!`][struct@Asyn]`[D => S,`[`Repeat<R>`]`]`
    /// function. `S` and `R` infers from the [`Asyn`][struct@Asyn] function body.
    ///
    /// If `func` resolves with [`Repeat::Continue`] it executes one more time.
    /// If `func` resolves with [`Repeat::Break(result)`], the loop stops and
    /// `result` passes to the next promise.
    pub fn repeat(state: S, func: Asyn![S => S, Repeat<R>]) -> Promise<S, R> {
        Promise::new(
            (state, func),
            asyn!(s => {
                let (state, func) = s.value;
                let next = func.clone();
                Promise::new(state, func).map(|state| (state, next)).then(asyn!(s, r => {
                    let (state, next) = s.value;
                    match r {
                        Repeat::Continue => PromiseResult::Await(Promise::repeat(state, next)),
                        Repeat::Break(result) => PromiseResult::Resolve(state, result)
                    }
                }))
            }),
        )
    }
}

impl<R: 'static> Promise<(), R> {
    /// Create stateless [resolve][PromiseResult::Resolve] with `R` result.
    pub fn resolve(result: R) -> PromiseResult<(), R> {
        PromiseResult::Resolve((), result)
    }
}

impl Promise<(), ()> {
    pub fn pass() -> PromiseResult<(), ()> {
        PromiseResult::Resolve((), ())
    }
    pub fn any<T: AnyPromises>(any: T) -> Promise<(), T::Result> {
        any.register()
    }
    pub fn all<T: AllPromises>(any: T) -> Promise<(), T::Result> {
        any.register()
    }
}

pub struct PromiseCommand<R> {
    id: PromiseId,
    result: R,
}

impl<R> PromiseCommand<R> {
    pub fn resolve(id: PromiseId, result: R) -> Self {
        PromiseCommand { id, result }
    }
}

impl<R: 'static + Send + Sync> Command for PromiseCommand<R> {
    fn apply(self, world: &mut World) {
        promise_resolve::<(), R>(world, self.id, (), self.result);
    }
}

impl<R: 'static, S: 'static> Command for Promise<S, R> {
    fn apply(self, world: &mut World) {
        promise_register::<S, R>(world, self)
    }
}

pub trait PromiseCommandsArg {}
impl PromiseCommandsArg for PromiseId {}
impl<S: 'static, R: 'static> PromiseCommandsArg for Promise<S, R> {}

pub struct PromiseCommands<'w, 's, 'a, T> {
    data: Option<T>,
    commands: Option<&'a mut Commands<'w, 's>>,
    finally: Option<fn(&'a mut Commands<'w, 's>, T)>,
}
impl<'w, 's, 'a> PromiseCommands<'w, 's, 'a, PromiseId> {
    pub fn resolve<R: 'static + Send + Sync>(&mut self, value: R) {
        let commands = mem::take(&mut self.commands).unwrap();
        let id = mem::take(&mut self.data).unwrap();
        commands.add(PromiseCommand::<R>::resolve(id, value));
    }
}
impl<'w, 's, 'a, T> Drop for PromiseCommands<'w, 's, 'a, T> {
    fn drop(&mut self) {
        let commands = mem::take(&mut self.commands);
        let data = mem::take(&mut self.data);
        if let Some(commands) = commands {
            if let Some(data) = data {
                if let Some(finally) = &self.finally {
                    finally(commands, data)
                }
            }
        }
    }
}

pub trait PromiseCommandsExtension<'w, 's, T> {
    fn promise<'a>(&'a mut self, promise: T) -> PromiseCommands<'w, 's, 'a, T>;
}

impl<'w, 's, S: 'static, F: FnOnce() -> S> PromiseCommandsExtension<'w, 's, F> for Commands<'w, 's> {
    /// Create [`PromiseLike<S, ()>`] chainable commands from default state constructor `|| -> S`
    fn promise<'a>(&'a mut self, arg: F) -> PromiseCommands<'w, 's, 'a, F> {
        PromiseCommands {
            data: Some(arg),
            commands: Some(self),
            finally: None,
        }
    }
}

impl<'w, 's> PromiseCommandsExtension<'w, 's, PromiseId> for Commands<'w, 's> {
    /// Create command for resolving promise by [`PromiseId`]
    fn promise<'a>(&'a mut self, arg: PromiseId) -> PromiseCommands<'w, 's, 'a, PromiseId> {
        PromiseCommands {
            data: Some(arg),
            commands: Some(self),
            finally: None,
        }
    }
}

impl<'w, 's, S: 'static, R: 'static> PromiseCommandsExtension<'w, 's, Promise<S, R>> for Commands<'w, 's> {
    /// Create [`PromiseLike<S, R>`] chainable commands from [`Promise<S, R>`]
    fn promise<'a>(&'a mut self, arg: Promise<S, R>) -> PromiseCommands<'w, 's, 'a, Promise<S, R>> {
        PromiseCommands {
            data: Some(arg),
            commands: Some(self),
            finally: Some(|commands, promise| commands.add(promise)),
        }
    }
}

pub struct PromiseChain<'w, 's, 'a, S: 'static, R: 'static> {
    commands: Option<&'a mut Commands<'w, 's>>,
    promise: Option<Promise<S, R>>,
}

impl<'w, 's, 'a, S: 'static, R: 'static> Drop for PromiseChain<'w, 's, 'a, S, R> {
    fn drop(&mut self) {
        if let Some(commands) = mem::take(&mut self.commands) {
            if let Some(promise) = mem::take(&mut self.promise) {
                commands.add(|world: &mut World| promise_register(world, promise))
            }
        }
    }
}

/// A wrapper for state that can be passed to asynchronous functions and promises.
///
/// The state wrapped by `PromiseState` is passed to the next function or promise in the chain.
/// You can create promises that have access to the current state with the `asyn` method:
///
/// ```ignore
/// let state = PromiseState::new(0);
/// let promise = state.asyn().timeout(1.0);
/// ```
///
/// The state can also be transformed using the `map` and `with` methods. For example, to change the
/// type of the state from `i32` to `String`, you can use `map`:
///
/// ```ignore
/// let state = PromiseState::new(42);
/// let new_state = state.map(|value| value.to_string());
/// ```
///
/// Or to set the state to a new value, you can use `with`:
///
/// ```ignore
/// let state = PromiseState::new(42);
/// let new_state = state.with("hello");
/// ```
///
/// Once you have a promise with access to the current state, you can chain it with other promises
/// that use the same state:
///
/// ```ignore
/// let state = PromiseState::new(0);
/// let promise = state.asyn().timeout(1.0)
///     .then(asyn!(state => {
///         state.value += 1;
///         state.asyn().timeout(1.0)
///     }))
///     .then(asyn!(state => {
///         info!("State value: {}", state.value);
///     }));
/// ```
pub struct PromiseState<S> {
    pub value: S,
}
impl<S: 'static> PromiseState<S> {
    /// Create a new `PromiseState` with the given initial value.
    pub fn new(value: S) -> PromiseState<S> {
        PromiseState { value }
    }

    /// Get access to stateful asyn operation.
    /// Promises returned by this operations will be
    /// associated with the current state:
    /// ```ignore
    /// fn setup(mut commands: Commands) {
    ///     commands.add(
    ///         Promise::from(0)
    ///         .then(asyn!(state => {
    ///             state.value += 1;
    ///             // state will be passed to the next promise
    ///             state.asyn().timeout(1.0)
    ///         }))
    ///         .then(asyn!(state => {
    ///             info!("State value: {}", state.value);
    ///         }))
    ///     )
    /// }
    /// ```
    pub fn asyn(self) -> AsynOps<S> {
        AsynOps(self.value)
    }

    /// Create a new `PromiseResult` with the given result.
    pub fn resolve<R>(self, result: R) -> PromiseResult<S, R> {
        PromiseResult::Resolve(self.value, result)
    }
    /// Create a new `PromiseResult` with no result.
    pub fn pass(self) -> PromiseResult<S, ()> {
        PromiseResult::Resolve(self.value, ())
    }

    /// Create a new `PromiseState` by mapping the current value with the given function.
    pub fn map<S2: 'static, F: FnOnce(S) -> S2>(self, map: F) -> PromiseState<S2> {
        PromiseState { value: map(self.value) }
    }

    /// Create a new `PromiseState` with the given value.
    pub fn with<S2: 'static>(self, value: S2) -> PromiseState<S2> {
        PromiseState { value }
    }

    /// Start a new promise chain with the given asynchronous function.
    pub fn start<S2: 'static, R2: 'static>(self, func: Asyn![S => S2, R2]) -> Promise<S2, R2> {
        Promise::new(self.value, func)
    }

    /// Start a new promise loop with the given asynchronous function.
    pub fn repeat<R2: 'static>(self, func: Asyn![S => S, Repeat<R2>]) -> Promise<S, R2> {
        Promise::repeat(self.value, func)
    }

    /// Combine the current promise chain with the given promises using the [`AnyPromises`] trait.
    pub fn any<A: AnyPromises>(self, any: A) -> Promise<S, A::Result> {
        any.register().with(self.value)
    }

    /// Combine the current promise chain with the given promises using the `AllPromises` trait.
    pub fn all<A: AllPromises>(self, all: A) -> Promise<S, A::Result> {
        all.register().with(self.value)
    }
}

impl<S: std::fmt::Display> std::fmt::Display for PromiseState<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.value.fmt(f)
    }
}

impl<S: 'static> std::ops::Deref for PromiseState<S> {
    type Target = S;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
impl<S: 'static> std::ops::DerefMut for PromiseState<S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
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
    fn register(self) -> Promise<(), Self::Result>;
}
pub trait AllPromises {
    type Result: 'static;
    fn register(self) -> Promise<(), Self::Result>;
}

impl<S: 'static, R: 'static> AnyPromises for Vec<Promise<S, R>> {
    type Result = (S, R);
    fn register(self) -> Promise<(), Self::Result> {
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
                            Promise::<(), ()>::register(
                                move |world, _id| {
                                    for (i, id) in ids.iter().enumerate() {
                                        if i != idx {
                                            promise_discard::<S, R>(world, *id);
                                        }
                                    }
                                    promise_resolve::<(), (S, R)>(world, any_id, (), (state, r))
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
                    promise_discard::<S, R>(world, id);
                }
            },
        )
    }
}

impl<S: 'static, R: 'static> AllPromises for Vec<Promise<S, R>> {
    type Result = Vec<(S, R)>;
    fn register(self) -> Promise<(), Self::Result> {
        let ids: Vec<PromiseId> = self.iter().map(|p| p.id).collect();
        let size = ids.len();
        Promise::register(
            move |world, any_id| {
                let value: Vec<Option<(S, R)>> = (0..size).map(|_| None).collect();
                let value = MutPtr::new(value);
                let mut idx = 0usize;
                for promise in self {
                    let value = value.clone();
                    promise_register(
                        world,
                        promise.map(move |s| (s, any_id, idx, value)).then(asyn!(|s, r| {
                            let (s, any_id, idx, mut value) = s.value;
                            Promise::<(), ()>::register(
                                move |world, _id| {
                                    value.get_mut()[idx] = Some((s, r));
                                    if value.get_ref().iter().all(|v| v.is_some()) {
                                        let value = value.get().into_iter().map(|v| v.unwrap()).collect();
                                        promise_resolve::<(), Vec<(S, R)>>(world, any_id, (), value)
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
                    promise_discard::<S, R>(world, id);
                }
            },
        )
    }
}

impl_any_promises! { 8 }
impl_all_promises! { 8 }

#[macro_export]
/// Generates signature an [`Asyn`][struct@Asyn] function wrapper. It allows you to specify
/// the input and output state and result types in the human-readable form: `Asyn![S, R => S2, R2]`.
macro_rules! Asyn {
    ($is:ty => $os:ty, $or:ty) => {
        $crate::Asyn<($crate::PromiseState<$is>, ()), impl 'static + Into<$crate::PromiseResult<$os, $or>>, impl $crate::PromiseParams>
    };
    ($is:ty, $ir:ty => $os:ty, $or:ty) => {
        $crate::Asyn<($crate::PromiseState<$is>, $ir), impl 'static + Into<$crate::PromiseResult<$os, $or>>, impl $crate::PromiseParams>
    }
}

pub struct Promises<S: 'static, R: 'static>(Vec<Promise<S, R>>);
impl<S: 'static, R: 'static> Promises<S, R> {
    pub fn any(self) -> Promise<(), (S, R)> {
        PromiseState::new(()).any(self.0)
    }
    pub fn all(self) -> Promise<(), Vec<(S, R)>> {
        PromiseState::new(()).all(self.0)
    }
}

pub trait PromisesExtension<S: 'static, R: 'static> {
    fn promise(self) -> Promises<S, R>;
}

impl<S: 'static, R: 'static, I: Iterator<Item = Promise<S, R>>> PromisesExtension<S, R> for I {
    fn promise(self) -> Promises<S, R> {
        Promises(self.collect())
    }
}

pub trait PromiseLikeBase<S: 'static, R: 'static>
where
    Self: Sized,
{
    type Promise<S2: 'static, R2: 'static>;
    /// Schedule the next [`Asyn![S, R => S2, R2]`][Asyn!] func invocation after current promise resolve.
    /// `S2` and `R2` infers from the `func` body
    fn then<S2: 'static, R2: 'static>(self, func: Asyn![S, R => S2, R2]) -> Self::Promise<S2, R2>;

    /// Create new [`PromiseLike<S, R>`] from previouse promise with result mapped by `map` from `R` to `R2`
    fn map_result<R2: 'static, F: 'static + FnOnce(R) -> R2>(self, map: F) -> Self::Promise<S, R2>;

    /// Create new [`PromiseLike<S, R2>`] from previouse promise with new result `R2`
    fn with_result<R2: 'static>(self, value: R2) -> Self::Promise<S, R2>;

    /// Create new [`PromiseLike<S2, R>`] from previouse promise with state mapped by `map` from `S` to `S2`
    fn map<S2: 'static, F: 'static + FnOnce(S) -> S2>(self, map: F) -> Self::Promise<S2, R>;

    /// Create new [`PromiseLike<S2, R>`] from previouse promise with state replaced with `S2`
    fn with<S2: 'static>(self, state: S2) -> Self::Promise<S2, R>;
}

pub trait PromiseLike<S: 'static>
where
    Self: Sized + PromiseLikeBase<S, ()>,
{
    /// Create new [`PromiseLike<S, R2>`] from default `S` state and  [`Asyn!`]`[D => S,`[`Repeat<R>`]`]` func.
    /// `R2` infers from the `func` body.
    /// If `func` resolves with `Repeat::Continue` it executes one more time.
    /// If `func` resolves with `Repeat::Break(result)`, the loop stops and
    /// `result` passes to the next promise.
    fn then_repeat<R2: 'static>(self, func: Asyn![S => S, Repeat<R2>]) -> Self::Promise<S, R2>;

    /// Create a new promise that resolves when all promises in the `all` parameter have resolved.
    fn all<A: 'static + AllPromises>(self, all: A) -> Self::Promise<S, A::Result>;

    /// Create a new promise that resolves when any of the promises in the `any` parameter have resolved.
    fn any<A: 'static + AnyPromises>(self, any: A) -> Self::Promise<S, A::Result>;
}
