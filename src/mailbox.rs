use std::collections::VecDeque;
use std::future::Future;
use std::num::NonZeroU64;
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
    reply_id: Option<NonZeroU64>,
    found: Option<Message>,
    messages: VecDeque<Message>,
}

impl MessageMailbox {
    /// Return message in FIFO order from mailbox.
    ///
    /// If function is called with a `tags` value different from None, it will only return the first
    /// message matching any of the tags.
    ///
    /// If no message exist, blocks until a message is received.
    pub async fn pop(&self, reply_id: Option<NonZeroU64>) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");

            // If a found message exists here, it means that the previous `.await` was canceled
            // after a `wake()` call. To not lose this message it should be put into the queue.
            if let Some(found) = mailbox.found.take() {
                mailbox.messages.push_back(found);
            }

            // When looking for specific tags, loop through all messages to check for it
            if let Some(reply_id) = reply_id {
                let index = mailbox
                    .messages
                    .iter()
                    .position(|m| m.is_reply_equal(reply_id));
                // If message matching tags is found, remove it.
                if let Some(index) = index {
                    return mailbox.messages.remove(index).expect("must exist");
                }
            } else {
                // If not looking for a specific tags try to pop the first message available.
                if let Some(message) = mailbox.messages.pop_front() {
                    return message;
                }
            }

            // Mark the tags to wait on.
            mailbox.reply_id = reply_id;
        }
        self.await
    }

    /// Pushes a message into the mailbox.
    ///
    /// If the message is being .awaited on, this call will immediately notify the waker that it's
    /// ready, otherwise it will push it at the end of the queue.
    pub fn push(&self, message: Message) {
        let mut mailbox = self.inner.lock().expect("only accessed by one process");
        // If waiting on a new message notify executor that it arrived.
        if let Some(waker) = mailbox.waker.take() {
            // If waiting on specific tags only notify if tags are matched, otherwise forward every message.
            // Note that because of the short-circuit rule in Rust it's safe to use `unwrap()` here.
            if mailbox.reply_id.is_none()
                || mailbox
                    .reply_id
                    .map(|rid| message.is_reply_equal(rid))
                    .unwrap_or(false)
            {
                mailbox.found = Some(message);
                waker.wake();
                return;
            } else {
                // Put the waker back if this is not the message we are looking for.
                mailbox.waker = Some(waker);
            }
        }
        // Otherwise put message into queue
        mailbox.messages.push_back(message);
    }
}

impl Future for &MessageMailbox {
    type Output = Message;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut mailbox = self.inner.lock().expect("only accessed by one process");
        if let Some(message) = mailbox.found.take() {
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
        task::{Context, Poll, Wake},
    };

    use crate::process::ProcessId;

    use super::{Message, MessageMailbox};

    fn msg_with_reply_id(proc_id: ProcessId, msg_id: u64, reply_id: NonZeroU64) -> Message {
        let msg_id = NonZeroU64::new(msg_id).unwrap();
        let mut msg = Message::new(msg_id, proc_id);
        msg.set_reply(reply_id);
        msg
    }

    #[async_std::test]
    async fn no_reply_signal_message() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        let message = Message::new_signal(msg_id, proc_id);
        mailbox.push(message);
        let result = mailbox.pop(None).await;
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
        let message = mailbox.pop(None).await;
        assert_eq!(message.is_reply_equal(rid), true);
    }

    #[async_std::test]
    async fn selective_receive_tag_signal_message() {
        let proc_id = ProcessId::new();

        let mailbox = MessageMailbox::default();

        let mut rids: Vec<NonZeroU64> = vec![];
        for i in 1..10 {
            let rid = NonZeroU64::new(i).unwrap();
            mailbox.push(msg_with_reply_id(proc_id, 100 + i, rid));
            rids.push(rid);
        }

        // shuffle some elements to get replys in a different order
        rids.swap(0, 2);
        rids.swap(1, 5);
        rids.swap(6, 7);

        for rid in rids {
            let message = mailbox.pop(Some(rid)).await;
            assert_eq!(message.is_reply_equal(rid), true);
        }
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
        let fut = mailbox.pop(None);
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
    fn waiting_on_reply_after_none() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        // "Awaiting" a specific reply and receiving a `None` message should not trigger the waker.
        let waker = FlagWaker(Arc::new(Mutex::new(false)));
        let waker_ref = waker.clone();
        let waker = &Arc::new(waker).into();
        let mut context = Context::from_waker(waker);
        // Request reply 1337
        let fut = mailbox.pop(Some(NonZeroU64::new(1337).unwrap()));
        let mut fut = Box::pin(fut);
        // First poll will block
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        assert_eq!(*waker_ref.0.lock().unwrap(), false);
        // Pushing a message without reply id 1337 should not trigger the waker
        mailbox.push(Message::new(msg_id, proc_id));
        assert_eq!(*waker_ref.0.lock().unwrap(), false);
        // Next poll will still not have the value with the reply id 1337
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        // Pushing another in the meantime should not remove the waker
        mailbox.push(Message::new(msg_id, proc_id));
        // Pushing a message with reply id 1337 should trigger the waker
        mailbox.push(msg_with_reply_id(
            proc_id,
            msg_id.get(),
            NonZeroU64::new(1337).unwrap(),
        ));
        assert_eq!(*waker_ref.0.lock().unwrap(), true);
        // Next poll will have the message ready
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_ready());
    }

    #[test]
    fn cancellation_safety() {
        let proc_id = ProcessId::new();
        let msg_id = NonZeroU64::new(1).unwrap();

        let mailbox = MessageMailbox::default();
        // Manually poll future
        let waker = FlagWaker(Arc::new(Mutex::new(false)));
        let waker_ref = waker.clone();
        let waker = &Arc::new(waker).into();
        let mut context = Context::from_waker(waker);
        let fut = mailbox.pop(None);
        let mut fut = Box::pin(fut);
        // First poll will block the future
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        assert_eq!(*waker_ref.0.lock().unwrap(), false);
        // Pushing a message with no reply id should call the waker()
        mailbox.push(Message::new(msg_id, proc_id));
        assert_eq!(*waker_ref.0.lock().unwrap(), true);
        // Dropping the future will cancel it
        drop(fut);
        // Next poll will not have the value with the reply id 1337
        let fut = mailbox.pop(Some(NonZeroU64::new(1337).unwrap()));
        tokio::pin!(fut);
        let result = fut.poll(&mut context);
        assert!(result.is_pending());
        // But will have the value None in the mailbox
        let fut = mailbox.pop(None);
        tokio::pin!(fut);
        let result = fut.poll(&mut context);
        match result {
            Poll::Ready(message) => {
                assert_eq!(message.is_reply(), false);
            }
            _ => panic!("Unexpected message"),
        }
    }
}
