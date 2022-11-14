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
    tags: Option<Vec<i64>>,
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
    pub async fn pop(&self, tags: Option<&[i64]>) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");

            // If a found message exists here, it means that the previous `.await` was canceled
            // after a `wake()` call. To not lose this message it should be put into the queue.
            if let Some(found) = mailbox.found.take() {
                mailbox.messages.push_back(found);
            }

            // When looking for specific tags, loop through all messages to check for it
            if let Some(tags) = tags {
                let index = mailbox.messages.iter().position(|x| {
                    // Only consider messages that also have a tag.
                    if let Some(tag) = x.tag() {
                        tags.contains(&tag)
                    } else {
                        false
                    }
                });
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
            mailbox.tags = tags.map(|tags| tags.into());
        }
        self.await
    }

    /// Similar to `pop`, but will assume right away that no message with this tags exists.
    ///
    /// Sometimes we know that the message we are waiting on can't have a particular tags already in
    /// the queue, so we can save ourself a search through the queue. This is often the case in a
    /// request/response architecture where we sent the tags to the remote server but couldn't have
    /// gotten it back yet.
    ///
    /// ### Safety
    ///
    /// It may not be clear right away why it's safe to skip looking through the queue. If we are
    /// waiting on a reply, didn't we already send the message and couldn't it already have been
    /// received and pushed into our queue?
    ///
    /// The way processes work is that they run a bit of code, *stop*, look for new signals/messages
    /// before running more code. This stop can only happen if there is an `.await` point in the
    /// code. Sending signals/messages is not an async task and we don't need to `.await` on it.
    /// When using this function we need to make sure that sending a specific tag and waiting on it
    /// doesn't contain any `.await` calls in-between. This implementation detail can be hidden
    /// inside of atomic host function calls so that end users don't need to worry about it.
    pub async fn pop_skip_search(&self, tags: Option<&[i64]>) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");

            // If a found message exists here, it means that the previous `.await` was canceled
            // after a `wake()` call. To not lose this message it should be put into the queue.
            if let Some(found) = mailbox.found.take() {
                mailbox.messages.push_back(found);
            }

            // Mark the tags to wait on.
            mailbox.tags = tags.map(|tags| tags.into());
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
            if mailbox.tags.is_none()
                || (message.tag().is_some()
                    && mailbox
                        .tags
                        .as_ref()
                        .unwrap()
                        .contains(&message.tag().unwrap()))
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

    /// Returns the number of messages currently available
    pub fn len(&self) -> usize {
        let mailbox = self.inner.lock().expect("only accessed by one process");

        mailbox.messages.len()
    }

    /// Returns true if the mailbox has no available messages
    pub fn is_empty(&self) -> bool {
        let mailbox = self.inner.lock().expect("only accessed by one process");

        mailbox.messages.is_empty()
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
        sync::{Arc, Mutex},
        task::{Context, Poll, Wake},
    };

    use super::{Message, MessageMailbox};

    #[tokio::test]
    async fn no_tags_signal_message() {
        let mailbox = MessageMailbox::default();
        let message = Message::LinkDied(None);
        mailbox.push(message);
        let result = mailbox.pop(None).await;
        match result {
            Message::LinkDied(None) => (),
            _ => panic!("Wrong message received"),
        }
    }

    #[tokio::test]
    async fn tag_signal_message() {
        let mailbox = MessageMailbox::default();
        let tag = 1337;
        let message = Message::LinkDied(Some(tag));
        mailbox.push(message);
        let message = mailbox.pop(None).await;
        assert_eq!(message.tag(), Some(tag));
    }

    #[tokio::test]
    async fn selective_receive_tag_signal_message() {
        let mailbox = MessageMailbox::default();
        let tag1 = 1;
        let tag2 = 2;
        let tag3 = 3;
        let tag4 = 4;
        let tag5 = 5;
        mailbox.push(Message::LinkDied(Some(tag1)));
        mailbox.push(Message::LinkDied(Some(tag2)));
        mailbox.push(Message::LinkDied(Some(tag3)));
        mailbox.push(Message::LinkDied(Some(tag4)));
        mailbox.push(Message::LinkDied(Some(tag5)));
        let message = mailbox.pop(Some(&[tag2])).await;
        assert_eq!(message.tag(), Some(tag2));
        let message = mailbox.pop(Some(&[tag1])).await;
        assert_eq!(message.tag(), Some(tag1));
        let message = mailbox.pop(Some(&[tag3])).await;
        assert_eq!(message.tag(), Some(tag3));
        // The only 2 left over are 4 & 5
        let message = mailbox.pop(None).await;
        assert_eq!(message.tag(), Some(tag4));
        let message = mailbox.pop(None).await;
        assert_eq!(message.tag(), Some(tag5));
    }

    #[tokio::test]
    async fn multiple_receive_tags_signal_message() {
        let mailbox = MessageMailbox::default();
        let tag1 = 1;
        let tag2 = 2;
        let tag3 = 3;
        let tag4 = 4;
        let tag5 = 5;
        mailbox.push(Message::LinkDied(Some(tag1)));
        mailbox.push(Message::LinkDied(Some(tag2)));
        mailbox.push(Message::LinkDied(Some(tag3)));
        mailbox.push(Message::LinkDied(Some(tag4)));
        mailbox.push(Message::LinkDied(Some(tag5)));
        let message = mailbox.pop(Some(&[tag2, tag1, tag3])).await;
        assert_eq!(message.tag(), Some(tag1));
        let message = mailbox.pop(Some(&[tag2, tag1, tag3])).await;
        assert_eq!(message.tag(), Some(tag2));
        let message = mailbox.pop(Some(&[tag2, tag1, tag3])).await;
        assert_eq!(message.tag(), Some(tag3));
        // The only 2 left over are 4 & 5
        let message = mailbox.pop(None).await;
        assert_eq!(message.tag(), Some(tag4));
        let message = mailbox.pop(None).await;
        assert_eq!(message.tag(), Some(tag5));
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
        let mailbox = MessageMailbox::default();
        // Sending a message with any tags to a mailbox that is "awaiting" a `None` tags should
        // trigger the waker and return the tags.
        let tags = Some(1337);
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
        assert!(!*waker_ref.0.lock().unwrap());
        // Pushing a message to the mailbox will call the waker
        mailbox.push(Message::LinkDied(tags));
        assert!(*waker_ref.0.lock().unwrap());
        // Next poll will return the value
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_ready());
    }

    #[test]
    fn waiting_on_tag_after_none() {
        let mailbox = MessageMailbox::default();
        // "Awaiting" a specific tags and receiving a `None` message should not trigger the waker.
        let waker = FlagWaker(Arc::new(Mutex::new(false)));
        let waker_ref = waker.clone();
        let waker = &Arc::new(waker).into();
        let mut context = Context::from_waker(waker);
        // Request tags 1337
        let fut = mailbox.pop(Some(&[1337]));
        let mut fut = Box::pin(fut);
        // First poll will block
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        assert!(!*waker_ref.0.lock().unwrap());
        // Pushing a message with the `None` tags should not trigger the waker
        mailbox.push(Message::LinkDied(None));
        assert!(!*waker_ref.0.lock().unwrap());
        // Next poll will still not have the value with the tags 1337
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_pending());
        // Pushing another None in the meantime should not remove the waker
        mailbox.push(Message::LinkDied(None));
        // Pushing a message with tags 1337 should trigger the waker
        mailbox.push(Message::LinkDied(Some(1337)));
        assert!(*waker_ref.0.lock().unwrap());
        // Next poll will have the message ready
        let result = fut.as_mut().poll(&mut context);
        assert!(result.is_ready());
    }

    #[test]
    fn cancellation_safety() {
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
        assert!(!*waker_ref.0.lock().unwrap());
        // Pushing a message with the `None` tags should call the waker()
        mailbox.push(Message::LinkDied(None));
        assert!(*waker_ref.0.lock().unwrap());
        // Dropping the future will cancel it
        drop(fut);
        // Next poll will not have the value with the tags 1337
        let fut = mailbox.pop(Some(&[1337]));
        tokio::pin!(fut);
        let result = fut.poll(&mut context);
        assert!(result.is_pending());
        // But will have the value None in the mailbox
        let fut = mailbox.pop(None);
        tokio::pin!(fut);
        let result = fut.poll(&mut context);
        match result {
            Poll::Ready(Message::LinkDied(tags)) => assert_eq!(tags, None),
            _ => panic!("Unexpected message"),
        }
    }
}
