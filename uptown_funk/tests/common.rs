use uptown_funk::{memory::Memory, Executor};

#[derive(Clone)]
pub struct SimpleExcutor {
    pub memory: Memory,
}

impl Executor for SimpleExcutor {
    type Return = ();

    fn memory(&self) -> Memory {
        self.memory.clone()
    }
}

pub struct Empty {}
