use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};

use crate::message::Message;

#[derive(Clone, Default)]
pub(crate) struct MessageMailbox {
    inner: Arc<Mutex<InnerMessageMailbox>>,
}

#[derive(Default)]
struct InnerMessageMailbox {
    waker: Option<Waker>,
    tag: Option<i64>,
    found: Option<Message>,
    messages: VecDeque<Message>,
}

impl MessageMailbox {
    // Return message in FIFO order from mailbox.
    //
    // If function is called with a `tag` value different from None, it will only return the first
    // message matching this tag.
    //
    // If no message exist, blocks until a message is received.
    pub(crate) async fn pop(&self, tag: Option<i64>) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");

            // When looking for a specific tag, loop through all messages to check for it
            if let Some(tag) = tag {
                let index = mailbox.messages.iter().position(|x| x.tag() == Some(tag));
                // If message matching tag is found, remove it.
                if let Some(index) = index {
                    return mailbox.messages.remove(index).expect("must exist");
                }
            } else {
                // If not looking for a specific tag try to pop the first message available.
                if let Some(message) = mailbox.messages.pop_front() {
                    return message;
                }
            }
            // Mark the tag to wait on
            mailbox.tag = tag;
        }
        self.await
    }

    // Similar to `pop`, but will assume right away that no message with this tag exists.
    //
    // Sometimes we know that the message we are waiting on can't have a particular tag already in
    // the queue, so we can save ourself a search through the queue. This is often the case in a
    // request/response architecture where we sent the tag to the remote server but couldn't have
    // gotten it back yet.
    //
    // ### Safety
    //
    // It may not be clear right away why it's safe to skip looking through the queue. If we are
    // waiting on a replay, didn't we already send the message and couldn't it already have been
    // received and pushed into our queue?
    //
    // The way processes work is that they run a bit of code, *stop*, look for new signals/messages
    // before running more code. This stop can only happen if there is an `.await` point in the
    // code. Sending signals/messages is not an async task and we don't need to `.await` on it.
    // When using this function we need to make sure that sending a specific tag and waiting for it
    // doesn't contain any `.await` calls in-between. This implementation detail can be hidden
    // inside of atomic host function calls so that end users don't need to worry about it.
    pub(crate) async fn pop_skip_search(&self, tag: Option<i64>) -> Message {
        // Mailbox lock must be released before .await
        {
            let mut mailbox = self.inner.lock().expect("only accessed by one process");
            mailbox.tag = tag;
        }
        self.await
    }

    pub(crate) fn push(&self, message: Message) {
        let mut mailbox = self.inner.lock().expect("only accessed by one process");
        // If waiting on a new message notify executor that it arrived.
        if let Some(waker) = mailbox.waker.take() {
            // If waiting on specific tag only notify if tag is matched, otherwise forward every message.
            // Note that because of the short-circuit rule it's safe to use `unwrap()` here.
            if mailbox.tag.is_none()
                || (message.tag().is_some() && message.tag().unwrap() == mailbox.tag.unwrap())
            {
                mailbox.found = Some(message);
                waker.wake();
                return;
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
    use super::{Message, MessageMailbox};

    #[tokio::test]
    async fn no_tag_message() {
        let mailbox = MessageMailbox::default();
        let message = Message::Signal(None);
        mailbox.push(message);
        mailbox.pop(None).await;
    }
}
