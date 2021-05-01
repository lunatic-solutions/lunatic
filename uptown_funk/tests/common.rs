use uptown_funk::{memory::Memory, Executor};

#[derive(Clone)]
pub struct SimpleExcutor {
    pub memory: Memory,
}

impl Executor for SimpleExcutor {
    fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

#[allow(dead_code)]
pub struct Empty;
