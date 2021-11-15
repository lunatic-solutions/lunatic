use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crate::message::Message;

/// The `MessageMailbox` is a data structure holding all messages of a process.
///
/// If a `Signal` of type `Message` is received it will be taken from the Signal queue and put into
/// this structure. The order of messages is preserved. This struct also implements the [`Future`]
/// trait and `pop()` operations can be awaited on if the queue is empty.
///
/// ## Safety
///
/// This should be cancellation safe and can be used inside `tokio::select!` statements:
/// https://docs.rs/tokio/1.10.0/tokio/macro.select.html#cancellation-safety
#[derive(Clone, Default)]
pub struct MessageMailbox {
    inner: Arc<Mutex<InnerMessageMailbox>>,
}

#[derive(Default)]
struct InnerMessageMailbox {
    waker: Option<Waker>,
    messages: VecDeque<Message>,
}

impl MessageMailbox {
    /// Return message in FIFO order from mailbox.
    ///
    /// If function is called with a `tags` value different from None, it will only return the first
    /// message matching any of the tags.
    ///
    /// If no message exist, blocks until a message is received.
    pub async fn pop(&self) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");
            if let Some(message) = mailbox.messages.pop_front() {
                return message;
            }
        }
        self.await
    }

    /// Pushes a message into the mailbox.
    ///
    /// If the message is being .awaited on, this call will immediately notify the waker that it's
    /// ready, otherwise it will push it at the end of the queue.
    pub fn push(&self, message: Message) {
        let mut mailbox = self.inner.lock().expect("only accessed by one process");
        mailbox.messages.push_back(message);

        // If waiting on a new message notify executor that it arrived.
        if let Some(waker) = mailbox.waker.take() {
            waker.wake();
        }
    }
}

impl Future for &MessageMailbox {
    type Output = Message;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut mailbox = self.inner.lock().expect("only accessed by one process");
        if let Some(message) = mailbox.messages.pop_front() {
            Poll::Ready(message)
        } else {
            mailbox.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::Future,
        num::NonZeroU64,
        sync::{Arc, Mutex},
        task::{Context, Wake},
    };

    use crate::process::ProcessId;

    use super::{Message, MessageMailbox};

    #[async_std::test]
    async fn no_reply_signal_message() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        let message = Message::new_signal(msg_id, proc_id);
        mailbox.push(message);
        let result = mailbox.pop().await;
        assert_eq!(result.is_signal(), true);
    }

    #[async_std::test]
    async fn tag_signal_message() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        let mut message = Message::new(msg_id, proc_id);
        let rid = NonZeroU64::new(1337).unwrap();
        message.set_reply(rid);
        mailbox.push(message);
        let message = mailbox.pop().await;
        assert_eq!(message.is_reply_equal(rid), true);
    }

    #[derive(Clone)]
    struct FlagWaker(Arc<Mutex<bool>>);
    impl Wake for FlagWaker {
        fn wake(self: Arc<Self>) {
            let mut called = self.0.lock().unwrap();
            *called = true;
        }
    }
    #[test]
    fn waiting_on_none_activates_waker() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        // Sending a message with any tags to a mailbox that is "awaiting" a `None` tags should
        // trigger the waker and return the tags.
        // Manually poll future
        let waker = FlagWaker(Arc::new(Mutex::new(false)));
        let waker_ref = waker.clone();
        let waker = &Arc::new(waker).into();
        let mut context = Context::from_waker(waker);
        // Request tags None
        let fut = mailbox.pop();
        let mut fut = Box::pin(fut);
        // First poll will block
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        assert_eq!(*waker_ref.0.lock().unwrap(), false);
        // Pushing a message to the mailbox will call the waker
        mailbox.push(Message::new(msg_id, proc_id));
        assert_eq!(*waker_ref.0.lock().unwrap(), true);
        // Next poll will return the value
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_ready());
    }

    #[test]
    fn cancellation_safety() {
        // TODO

        //let proc_id = ProcessId::new();
        //let msg_id = NonZeroU64::new(1).unwrap();

        //let mailbox = MessageMailbox::default();
        //// Manually poll future
        //let waker = FlagWaker(Arc::new(Mutex::new(false)));
        //let waker_ref = waker.clone();
        //let waker = &Arc::new(waker).into();
        //let mut context = Context::from_waker(waker);
        //let fut = mailbox.pop(None);
        //let mut fut = Box::pin(fut);
        //// First poll will block the future
        //let result = fut.as_mut().poll(&mut context);
        //assert!(result.is_pending());
        //assert_eq!(*waker_ref.0.lock().unwrap(), false);
        //// Pushing a message with no reply id should call the waker()
        //mailbox.push(Message::new(msg_id, proc_id));
        //assert_eq!(*waker_ref.0.lock().unwrap(), true);
        //// Dropping the future will cancel it
        //drop(fut);
        //// Next poll will not have the value with the reply id 1337
        //let fut = mailbox.pop(Some(NonZeroU64::new(1337).unwrap()));
        //tokio::pin!(fut);
        //let result = fut.poll(&mut context);
        //assert!(result.is_pending());
        //// But will have the value None in the mailbox
        //let fut = mailbox.pop(None);
        //tokio::pin!(fut);
        //let result = fut.poll(&mut context);
        //match result {
        //    Poll::Ready(message) => {
        //        assert_eq!(message.is_reply(), false);
        //    }
        //    _ => panic!("Unexpected message"),
        //}
    }
}
