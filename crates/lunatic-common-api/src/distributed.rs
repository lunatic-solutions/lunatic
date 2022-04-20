use crate::actor::{ActorHandle, Request};

#[derive(Debug, Clone)]
pub struct Spawn {
    pub node_id: u64,
    pub module_id: u64,
}

impl Request for Spawn {
    type Response = u64;
}

#[derive(Clone)]
pub struct DistributedInterface {
    pub spawn: ActorHandle<Spawn>,
}
