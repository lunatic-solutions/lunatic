use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime};

use log::debug;
use log::error;

type Ptr = u32;
type Size = u32;
const HISTORY_CAPACITY: usize = 100_000;

thread_local! {
    static STATE: RefCell<HeapProfilerState> = RefCell::new(HeapProfilerState::new());
}

struct HeapProfilerState {
    memory: HashMap<Ptr, Size>,
    started: SystemTime,
    heap_history: VecDeque<(i32, Duration)>,
}

impl HeapProfilerState {
    pub fn new() -> Self {
        Self {
            memory: HashMap::new(),
            started: SystemTime::now(),
            heap_history: VecDeque::new(),
        }
    }

    fn history_push(&mut self, change: i32) {
        if self.heap_history.len() == HISTORY_CAPACITY {
            // if HISTRY_CAPACITY > 0 this should be safe
            self.heap_history.pop_front().unwrap();
        }
        match self.started.elapsed() {
            Ok(duration) => self.heap_history.push_back((change, duration)),
            Err(e) => error!("profiler_history_push: {}", e),
        }
    }

    // Write heap profile history to a file. Format:
    //
    // #time/sec heap/byte
    // 0.1       10
    // 0.2       15
    // ...
    #[allow(dead_code)]
    pub fn write_dat(&self, fd: &mut File) -> std::io::Result<()> {
        let mut graph = Vec::new();
        let mut heap_size: u64 = 0;
        writeln!(&mut graph, "#time/sec heap/byte")?;
        // write initial entry for plotting
        writeln!(&mut graph, "0 0").unwrap();
        self.heap_history.iter().for_each(|(heap, duration)| {
            if *heap < 0 {
                // TODO check for overflow
                heap_size -= (-*heap) as u64;
            } else {
                heap_size += *heap as u64;
            }
            writeln!(&mut graph, "{} {}", duration.as_secs_f64(), heap_size).unwrap();
        });
        fd.write_all(&graph)
    }
}

#[export_name = "aligned_alloc_profiler"]
pub extern "C" fn aligned_alloc_profiler(_self: u32, size: Size, ptr: Ptr) {
    malloc_profiler(size, ptr);
}

#[export_name = "malloc_profiler"]
pub extern "C" fn malloc_profiler(size: Size, ptr: Ptr) {
    debug!("heap_profiler: malloc({}) -> {}", size, ptr);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        state.memory.insert(ptr, size);
        state.history_push(size as i32);
    });
}

#[export_name = "calloc_profiler"]
pub extern "C" fn calloc_profiler(len: Size, elem_size: Size, ptr: Ptr) {
    debug!("heap_profiler: calloc({},{}) -> {}", len, elem_size, ptr);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        let size = len * elem_size;
        state.memory.insert(ptr, size);
        state.history_push(size as i32);
    });
}

#[export_name = "realloc_profiler"]
pub extern "C" fn realloc_profiler(old_ptr: Ptr, size: Size, new_ptr: Ptr) {
    debug!(
        "heap_profiler: realloc({},{}) -> {}",
        old_ptr, size, new_ptr
    );
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        // realloc spec: if ptr is null then the call is equivalent  to malloc(size)
        if old_ptr == 0 {
            state.memory.insert(new_ptr, size);
            state.history_push(size as i32);
        } else {
            match state.memory.remove(&old_ptr) {
                Some(removed_size) => {
                    state.memory.insert(new_ptr, size);
                    let size_delta = size as i32 - removed_size as i32;
                    state.history_push(size_delta);
                }
                None => error!(
                    "heap_profiler: can't reallocate, pointer {} doesn't exist",
                    old_ptr
                ),
            };
        }
    });
}

#[export_name = "free_profiler"]
pub extern "C" fn free_profiler(ptr: Ptr) {
    debug!("heap_profiler: free({})", ptr);
    STATE.with(|state| {
        let mut state = state.borrow_mut();
        // free spec: if ptr is null no action is performed
        if ptr != 0 {
            match state.memory.remove(&ptr) {
                Some(size) => {
                    state.history_push(-(size as i32));
                }
                None => error!("heap_profiler: can't free, pointer {} doesn't exist", ptr),
            };
        }
    });
}
