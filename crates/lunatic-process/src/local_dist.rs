use async_std::channel;
use async_std::task;
use lunatic_common_api::{
    actor::{Actor, Responder},
    distributed::{DistributedInterface, Spawn},
};

#[derive(Clone)]
struct DummyDistributed {}

pub fn dummy_distributed() -> DistributedInterface {
    let d = DummyDistributed {};
    DistributedInterface {
        spawn: d.spawn(),
    }
}

impl Actor<Spawn> for DummyDistributed {
    fn spawn_task(self, receiver: channel::Receiver<(Spawn, Responder<Spawn>)>) {
        task::spawn(async move {
            while let Ok((_req, resp)) = receiver.recv().await {
                resp.respond(0).await
            }
        });
    }
}
