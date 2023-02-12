//! Make `http` requests asyncroniusly via [`ehttp`](https://docs.rs/ehttp/)
use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use bevy::utils::HashMap;
pub use ehttp::Response;
use futures_lite::future;
use pecs_core::{AsyncOps, Promise, PromiseCommand, PromiseId, PromiseResult};

pub struct PromiseHttpPlugin;
impl Plugin for PromiseHttpPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Requests>();
        app.add_system(process_requests);
    }
}

pub struct Request(ehttp::Request);
impl Request {
    pub(crate) fn new() -> Self {
        Self(ehttp::Request::get(""))
    }
    pub fn url<U: ToString>(mut self, url: U) -> Self {
        self.0.url = url.to_string();
        self
    }
    pub fn method<M: ToString>(mut self, method: M) -> Self {
        self.0.method = method.to_string();
        self
    }
    pub fn body<B: Into<Vec<u8>>>(mut self, body: B) -> Self {
        self.0.body = body.into();
        self
    }
    pub fn header<K: ToString, V: ToString>(mut self, key: K, value: V) -> Self {
        self.0.headers.insert(key.to_string(), value.to_string());
        self
    }
    pub fn send(self) -> Promise<(), Result<Response, String>> {
        Promise::register(
            |world, id| {
                let task = AsyncComputeTaskPool::get().spawn(async move { ehttp::fetch_blocking(&self.0) });
                world.resource_mut::<Requests>().insert(id, task);
            },
            |world, id| {
                world.resource_mut::<Requests>().remove(&id);
            },
        )
    }
}

pub struct StatefulRequest<S>(S, Request);
impl<S: 'static> StatefulRequest<S> {
    pub(crate) fn new(state: S) -> Self {
        Self(state, Request::new())
    }
    pub fn url<U: ToString>(mut self, url: U) -> Self {
        self.1 = self.1.url(url);
        self
    }
    pub fn method<M: ToString>(mut self, method: M) -> Self {
        self.1 = self.1.method(method);
        self
    }
    pub fn header<K: ToString, V: ToString>(mut self, key: K, value: V) -> Self {
        self.1 = self.1.header(key, value);
        self
    }
    pub fn body<B: Into<Vec<u8>>>(mut self, body: B) -> Self {
        self.1 = self.1.body(body);
        self
    }
    pub fn send(self) -> Promise<S, Result<ehttp::Response, String>> {
        self.1.send().map(move |_| self.0)
        // PromiseResult::Await(self.1.send()).with(self.0)
    }
}

pub struct Http<S>(S);

impl<S: 'static> Http<S> {
    pub fn get<U: ToString>(self, url: U) -> StatefulRequest<S> {
        StatefulRequest::new(self.0).method("GET").url(url)
    }
    pub fn post<U: ToString>(self, url: U) -> StatefulRequest<S> {
        StatefulRequest::new(self.0).method("POST").url(url)
    }
    pub fn request<M: ToString, U: ToString>(self, method: M, url: U) -> StatefulRequest<S> {
        StatefulRequest::new(self.0).method(method).url(url)
    }
}
pub trait HttpOpsExtension<S> {
    fn http(self) -> Http<S>;
}
impl<S> HttpOpsExtension<S> for AsyncOps<S> {
    fn http(self) -> Http<S> {
        Http(self.0)
    }
}

#[derive(Resource, Deref, DerefMut, Default)]
pub struct Requests(HashMap<PromiseId, Task<Result<Response, String>>>);

pub fn process_requests(mut requests: ResMut<Requests>, mut commands: Commands) {
    requests.drain_filter(|promise, mut task| {
        if let Some(response) = future::block_on(future::poll_once(&mut task)) {
            commands.add(PromiseCommand::resolve(*promise, response));
            true
        } else {
            false
        }
    });
}

impl From<Request> for PromiseResult<(), Result<Response, String>> {
    fn from(value: Request) -> Self {
        PromiseResult::Await(value.send())
    }
}

impl<S: 'static> From<StatefulRequest<S>> for PromiseResult<S, Result<Response, String>> {
    fn from(value: StatefulRequest<S>) -> Self {
        PromiseResult::Await(value.send())
    }
}

pub mod asyn {
    pub fn get<T: ToString>(url: T) -> super::Request {
        super::Request::new().method("GET").url(url)
    }
    pub fn post<U: ToString>(url: U) -> super::Request {
        super::Request::new().method("POST").url(url)
    }
    pub fn request<M: ToString, U: ToString>(method: M, url: U) -> super::Request {
        super::Request::new().method(method).url(url)
    }
}
