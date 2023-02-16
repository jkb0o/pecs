use crate::*;

impl<S: 'static, R: 'static> PromiseLikeBase<S, R> for Promise<S, R> {
    type Promise<S2: 'static, R2: 'static> = Promise<S2, R2>;
    fn then<S2: 'static, R2: 'static>(mut self, func: Asyn![S, R => S2, R2]) -> Promise<S2, R2> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<S2, R2>(world, id);
        }));
        self.resolve = Some(Box::new(move |world, state, result| {
            let pr = func.run((PromiseState::new(state), result), world).into();
            match pr {
                PromiseResult::Resolve(s, r) => promise_resolve::<S2, R2>(world, id, s, r),
                PromiseResult::Await(mut p) => {
                    if p.resolve.is_some() {
                        error!(
                            "Misconfigured {}<{}, {}>, resolve already defined",
                            p.id,
                            type_name::<S2>(),
                            type_name::<R2>(),
                        );
                        return;
                    }
                    p.resolve = Some(Box::new(move |world, s, r| {
                        promise_resolve::<S2, R2>(world, id, s, r);
                    }));
                    promise_register::<S2, R2>(world, p);
                }
            }
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<S, R>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
        }
    }

    fn map_result<R2: 'static, F: 'static + FnOnce(R) -> R2>(mut self, map: F) -> Self::Promise<S, R2> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<S, R2>(world, id);
        }));
        self.resolve = Some(Box::new(move |world, state, result| {
            let result = map(result);
            promise_resolve::<S, R2>(world, id, state, result);
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<S, R>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
        }
    }
    fn with_result<R2: 'static>(self, value: R2) -> Self::Promise<S, R2> {
        self.map_result(|_| value)
    }
    fn map<S2: 'static, F: 'static + FnOnce(S) -> S2>(mut self, map: F) -> Self::Promise<S2, R> {
        let id = PromiseId::new();
        let discard = mem::take(&mut self.discard);
        let self_id = self.id;
        self.discard = Some(Box::new(move |world, _id| {
            promise_discard::<S2, R>(world, id);
        }));
        self.resolve = Some(Box::new(move |world, state, result| {
            let state = map(state);
            promise_resolve::<S2, R>(world, id, state, result);
        }));
        Promise {
            id,
            register: Some(Box::new(move |world, _id| {
                promise_register::<S, R>(world, self);
            })),
            discard: Some(Box::new(move |world, _id| {
                if let Some(discard) = discard {
                    discard(world, self_id);
                }
            })),
            resolve: None,
        }
    }
    fn with<S2: 'static>(self, state: S2) -> Self::Promise<S2, R> {
        self.map(|_| state)
    }
}
impl<S: 'static> PromiseLike<S> for Promise<S, ()> {
    fn then_repeat<R2: 'static>(self, func: Asyn![S => S, Repeat<R2>]) -> Self::Promise<S, R2> {
        self.map(|state| (state, func)).then(asyn!(s, _ => {
            let (state, func) = s.value;
            Promise::repeat(state, func)
        }))
    }
    fn all<A: 'static + AllPromises>(self, all: A) -> Self::Promise<S, A::Result> {
        self.map(|s| (s, all)).then(asyn!(state => {
            let (state, all) = state.value;
            all.register().with(state)
        }))
    }

    fn any<A: 'static + AnyPromises>(self, any: A) -> Self::Promise<S, A::Result> {
        self.map(|s| (s, any)).then(asyn!(state => {
            let (state, any) = state.value;
            any.register().with(state)
        }))
    }
}

impl<'w, 's, 'a, S: 'static, F: FnOnce() -> S> PromiseLikeBase<S, ()> for PromiseCommands<'w, 's, 'a, F> {
    type Promise<S2: 'static, R2: 'static> = PromiseChain<'w, 's, 'a, S2, R2>;
    fn then<S2: 'static, R2: 'static>(mut self, func: Asyn![S => S2, R2]) -> Self::Promise<S2, R2> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::new(new_state(), asyn!(s => s)).then(func)),
        }
    }
    fn map_result<R2: 'static, M: 'static + FnOnce(()) -> R2>(mut self, map: M) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::new(
                (new_state(), map(())),
                asyn!(s => {
                    let (state, result) = s.value;
                    PromiseResult::Resolve(state, result)
                }),
            )),
        }
    }
    fn with_result<R2: 'static>(self, value: R2) -> Self::Promise<S, R2> {
        self.map_result(|_| value)
    }
    fn map<S2: 'static, M: 'static + FnOnce(S) -> S2>(mut self, map: M) -> Self::Promise<S2, ()> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::new(map(new_state()), asyn!(s => s))),
        }
    }
    fn with<S2: 'static>(self, state: S2) -> Self::Promise<S2, ()> {
        self.map(|_| state)
    }
}

impl<'w, 's, 'a, S: 'static, F: FnOnce() -> S> PromiseLike<S> for PromiseCommands<'w, 's, 'a, F> {
    fn then_repeat<R2: 'static>(mut self, func: Asyn![S => S, Repeat<R2>]) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::repeat(new_state(), func)),
        }
    }
    fn all<A: 'static + AllPromises>(mut self, all: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::all(all).with(new_state())),
        }
    }
    fn any<A: 'static + AnyPromises>(mut self, any: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands);
        let new_state = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(Promise::any(any).with(new_state())),
        }
    }
}

impl<'w, 's, 'a, S: 'static, R: 'static> PromiseLikeBase<S, R> for PromiseCommands<'w, 's, 'a, Promise<S, R>> {
    type Promise<S2: 'static, R2: 'static> = PromiseChain<'w, 's, 'a, S2, R2>;
    fn then<S2: 'static, R2: 'static>(mut self, func: Asyn![S, R => S2, R2]) -> Self::Promise<S2, R2> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.then(func)),
        }
    }
    fn map_result<R2: 'static, F: 'static + FnOnce(R) -> R2>(mut self, map: F) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.map_result(map)),
        }
    }
    fn with_result<R2: 'static>(self, value: R2) -> Self::Promise<S, R2> {
        self.map_result(|_| value)
    }
    fn map<S2: 'static, F: 'static + FnOnce(S) -> S2>(mut self, m: F) -> Self::Promise<S2, R> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.map(m)),
        }
    }
    fn with<S2: 'static>(self, state: S2) -> Self::Promise<S2, R> {
        self.map(|_| state)
    }
}
impl<'w, 's, 'a, S: 'static> PromiseLike<S> for PromiseCommands<'w, 's, 'a, Promise<S, ()>> {
    fn then_repeat<R2: 'static>(mut self, func: Asyn![S => S, Repeat<R2>]) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.then_repeat(func)),
        }
    }
    fn all<A: 'static + AllPromises>(mut self, all: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.all(all)),
        }
    }
    fn any<A: 'static + AnyPromises>(mut self, any: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands);
        let promise = mem::take(&mut self.data).unwrap();
        PromiseChain {
            commands,
            promise: Some(promise.any(any)),
        }
    }
}

impl<'w, 's, 'a, S: 'static, R: 'static> PromiseLikeBase<S, R> for PromiseChain<'w, 's, 'a, S, R> {
    type Promise<S2: 'static, R2: 'static> = PromiseChain<'w, 's, 'a, S2, R2>;
    fn then<S2: 'static, R2: 'static>(mut self, func: Asyn![S, R => S2, R2]) -> Self::Promise<S2, R2> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.then(func)),
        }
    }
    fn map_result<R2: 'static, F: 'static + FnOnce(R) -> R2>(mut self, map: F) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.map_result(map)),
        }
    }
    fn with_result<R2: 'static>(self, value: R2) -> Self::Promise<S, R2> {
        self.map_result(|_| value)
    }
    fn map<S2: 'static, F: 'static + FnOnce(S) -> S2>(mut self, map: F) -> Self::Promise<S2, R> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.map(map)),
        }
    }
    fn with<S2: 'static>(self, state: S2) -> Self::Promise<S2, R> {
        self.map(|_| state)
    }
}

impl<'w, 's, 'a, S: 'static> PromiseLike<S> for PromiseChain<'w, 's, 'a, S, ()> {
    fn then_repeat<R2: 'static>(mut self, func: Asyn![S => S, Repeat<R2>]) -> Self::Promise<S, R2> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.then_repeat(func)),
        }
    }
    fn all<A: 'static + AllPromises>(mut self, all: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.all(all)),
        }
    }
    fn any<A: 'static + AnyPromises>(mut self, any: A) -> Self::Promise<S, A::Result> {
        let commands = mem::take(&mut self.commands).unwrap();
        let promise = mem::take(&mut self.promise).unwrap();
        PromiseChain {
            commands: Some(commands),
            promise: Some(promise.any(any)),
        }
    }
}
