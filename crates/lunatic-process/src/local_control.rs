use std::sync::Arc;

use async_std::{channel::Receiver, task};
use dashmap::DashMap;
use lunatic_common_api::{
    actor::{Actor, Responder},
    control::{ControlInterface, GetModule, GetNodeIds},
};

#[derive(Clone)]
struct LocalControl {
    modules: Arc<DashMap<u64, Vec<u8>>>,
}

impl LocalControl {
    pub fn new() -> Self {
        Self {
            modules: Arc::new(DashMap::new()),
        }
    }
}

impl Actor<GetNodeIds> for LocalControl {
    fn spawn_task(self, receiver: Receiver<(GetNodeIds, Responder<GetNodeIds>)>) {
        task::spawn(async move {
            while let Ok((_, resp)) = receiver.recv().await {
                resp.respond(vec![1]).await;
            }
        });
    }
}

impl Actor<GetModule> for LocalControl {
    fn spawn_task(self, receiver: Receiver<(GetModule, Responder<GetModule>)>) {
        task::spawn(async move {
            while let Ok((req, resp)) = receiver.recv().await {
                resp.respond(self.modules.get(&req.module_id).map(|e| e.clone()))
                    .await
            }
        });
    }
}

pub fn local_control() -> ControlInterface {
    let ctrl = LocalControl::new();
    ControlInterface {
        node_id: 1,
        get_module: ctrl.clone().spawn(),
        get_nodes: ctrl.clone().spawn(),
        register_module: ctrl.spawn(),
    }
}
