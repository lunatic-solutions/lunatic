use async_std::{
    channel::{unbounded, Receiver, Sender},
    task,
};

struct Request<A, B> {
    pub message: A,
    send_response_to: Sender<B>,
}

impl<A, B> Request<A, B> {
    pub async fn respond(&self, resp: B) {
        self.send_response_to.send(resp).await.ok();
    }
}

#[derive(Clone)]
struct ActorHandle<A, B> {
    sender: Sender<Request<A, B>>,
}

impl<A, B> ActorHandle<A, B> {
    pub async fn send(&self, request: A) -> Receiver<B> {
        let (sender, receiver) = unbounded();
        let request = Request {
            message: request,
            send_response_to: sender,
        };
        self.sender.send(request).await.unwrap();
        receiver
    }
}

trait Actor<A, B>: Sized {
    fn spawn(self) -> ActorHandle<A, B>;
}

struct Adder {
    sum: u32,
}

impl Adder {
    pub fn new() -> Self {
        Adder { sum: 0 }
    }
}

impl Actor<u32, u32> for Adder {
    fn spawn(mut self) -> ActorHandle<u32, u32> {
        let (sender, receiver) = unbounded::<Request<u32, u32>>();
        task::spawn(async move {
            while let Ok(req) = receiver.recv().await {
                self.sum += req.message;
                req.respond(self.sum).await;
            }
            // channel is empty and closed when recv() returns Err
        });
        ActorHandle { sender }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[async_std::test]
    async fn test_simple_case() {
        let adder = Adder::new().spawn();
        let sum = adder.send(1).await.recv().await.unwrap();
        assert_eq!(sum, 1);

        let sum = adder.send(5).await.recv().await.unwrap();
        assert_eq!(sum, 6);
    }
}
