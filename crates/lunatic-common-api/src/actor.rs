use async_std::channel;

pub trait Request {
    type Response: Send;
}

// TODO: change Sender to tokio OneShotChannel or AsyncCell
pub struct Responder<A: Request>(channel::Sender<A::Response>);

impl<A: Request> Responder<A> {
    pub async fn respond(&self, resp: A::Response) {
        self.0.send(resp).await.ok();
    }
}

pub struct Receiver<A: Request>(channel::Receiver<A::Response>);

impl<A: Request> Receiver<A> {
    pub async fn recv(&self) -> A::Response {
        self.0.recv().await.unwrap()
    }
}

fn responder_receiver<A: Request>() -> (Responder<A>, Receiver<A>) {
    let (responder, receiver) = channel::unbounded();
    (Responder(responder), Receiver(receiver))
}

#[derive(Clone)]
pub struct ActorHandle<A: Request> {
    sender: channel::Sender<(A, Responder<A>)>,
}

impl<A: Request> ActorHandle<A> {
    pub async fn send(&self, request: A) -> Receiver<A> {
        let (responder, receiver) = responder_receiver();
        self.sender.send((request, responder)).await.unwrap();
        receiver
    }

    pub async fn call(&self, request: A) -> A::Response {
        self.send(request).await.recv().await
    }
}

pub trait Actor<A: Request>: Sized {
    // Usually need to implement two-line boilerplate
    // task::spawn(async move { while let Ok((req, resp)) = receiver.recv().await { } }
    // if async traits were possible then you would be able to implement only async handler function
    fn spawn_task(self, receiver: channel::Receiver<(A, Responder<A>)>);

    fn spawn(self) -> ActorHandle<A> {
        let (sender, receiver) = channel::unbounded();
        self.spawn_task(receiver);
        ActorHandle { sender }
    }
}

pub trait ActorCtx<T: Request> {
    fn actor(&self) -> ActorHandle<T>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_std::task;
    struct Adder {
        sum: u32,
    }

    impl Adder {
        pub fn new() -> Self {
            Adder { sum: 0 }
        }
    }

    struct Add(pub u32);

    impl Request for Add {
        type Response = u32;
    }

    impl Actor<Add> for Adder {
        fn spawn_task(mut self, receiver: channel::Receiver<(Add, Responder<Add>)>) {
            task::spawn(async move {
                while let Ok((req, resp)) = receiver.recv().await {
                    self.sum += req.0;
                    resp.respond(self.sum).await;
                }
                // channel is empty and closed when recv() returns Err
            });
        }
    }

    #[async_std::test]
    async fn test_simple_case() {
        let adder = Adder::new().spawn();
        let sum = adder.send(Add(1)).await.recv().await;
        assert_eq!(sum, 1);

        let sum = adder.send(Add(5)).await.recv().await;
        assert_eq!(sum, 6);
    }
}
