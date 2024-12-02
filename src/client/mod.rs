use std::{future::Future, marker::PhantomData};

use crate::bot::{BotService, Runtime};

#[cfg(feature = "console_client")]
pub mod console;

#[cfg(feature = "discord_client")]
pub mod discord;

pub trait ServiceVisitor: Send + Sync {
    type Runtime: Runtime;
    fn visit(&self, service: &impl BotService<Self::Runtime>) -> impl Future<Output = ()> + Send;
}

pub trait ServiceList<R: Runtime>: Sized + Send + Sync {
    fn append<S: BotService<R>>(self, s: S) -> ListCons<R, S, Self> {
        ListCons {
            service: s,
            child: self,
            _phantom: PhantomData,
        }
    }
    fn visit(&self, visitor: &impl ServiceVisitor<Runtime = R>) -> impl Future<Output = ()> + Send;
}

pub struct ListNil;
impl<R: Runtime> ServiceList<R> for ListNil {
    async fn visit(&self, _visitor: &impl ServiceVisitor) {}
}

pub struct ListCons<R: Runtime, S: BotService<R>, C: ServiceList<R>> {
    service: S,
    child: C,
    _phantom: PhantomData<fn() -> R>,
}
impl<R: Runtime, S: BotService<R>, C: ServiceList<R>> ServiceList<R> for ListCons<R, S, C> {
    async fn visit(&self, visitor: &impl ServiceVisitor<Runtime = R>) {
        self.child.visit(visitor).await;
        visitor.visit(&self.service).await;
    }
}
