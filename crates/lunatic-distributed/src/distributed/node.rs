use std::sync::Arc;

#[derive(Clone)]
pub struct Node {
    _inner: Arc<InnerNode>,
}

struct InnerNode {}

#[allow(clippy::new_without_default)]
impl Node {
    pub fn new() -> Node {
        Node {
            _inner: Arc::new(InnerNode {}),
        }
    }
}
