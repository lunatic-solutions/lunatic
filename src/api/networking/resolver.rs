use std::{cell::RefCell, io, rc::Rc, vec::IntoIter};
use uptown_funk::{Executor, FromWasm, ToWasm};

use super::api::TcpState;

#[derive(Clone)]
pub struct Resolver {
    iter: Rc<RefCell<IntoIter<smol::net::SocketAddr>>>,
}

impl Resolver {
    pub async fn resolve(name: &str) -> Result<Self, io::Error> {
        let resolved = smol::net::resolve(name).await?;
        Ok(Resolver {
            iter: Rc::new(RefCell::new(resolved.into_iter())),
        })
    }

    pub fn next(&self) -> Option<smol::net::SocketAddr> {
        self.iter.as_ref().borrow_mut().next()
    }
}

impl FromWasm for Resolver {
    type From = u32;
    type State = TcpState;

    fn from(
        state: &mut Self::State,
        _: &impl Executor,
        resolver_id: u32,
    ) -> Result<Self, uptown_funk::Trap>
    where
        Self: Sized,
    {
        match state.resolvers.get(resolver_id) {
            Some(resolver) => Ok(resolver.clone()),
            None => Err(uptown_funk::Trap::new("TcpListener not found")),
        }
    }
}

pub enum ResolverResult {
    Ok(Resolver),
    Err(String),
}

impl ToWasm for ResolverResult {
    type To = u32;
    type State = TcpState;

    fn to(
        state: &mut Self::State,
        _: &impl Executor,
        result: Self,
    ) -> Result<u32, uptown_funk::Trap> {
        match result {
            ResolverResult::Ok(resolver) => Ok(state.resolvers.add(resolver)),
            ResolverResult::Err(_err) => Ok(0),
        }
    }
}
