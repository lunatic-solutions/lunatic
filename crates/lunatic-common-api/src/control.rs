use crate::actor::{ActorHandle, Request};

#[derive(Debug, Clone)]
pub struct RegisterModule {
    pub bytes: Vec<u8>,
}

impl Request for RegisterModule {
    type Response = u64;
}

#[derive(Debug, Copy, Clone)]
pub struct GetModule {
    pub module_id: u64,
}

impl Request for GetModule {
    type Response = Option<Vec<u8>>;
}

#[derive(Debug, Copy, Clone)]
pub struct GetNodeIds {}

impl Request for GetNodeIds {
    type Response = Vec<u64>;
}

#[derive(Clone)]
pub struct ControlInterface {
    pub node_id: u64,
    pub get_module: ActorHandle<GetModule>,
    pub register_module: ActorHandle<GetNodeIds>,
    pub get_nodes: ActorHandle<GetNodeIds>,
}
