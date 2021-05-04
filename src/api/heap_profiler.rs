use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Write;
use std::time::{Duration, SystemTime};
use uptown_funk::host_functions;

use log::debug;
use log::error;

type Ptr = u32;
type Size = u32;
const HISTORY_CAPACITY: usize = 100_000;

pub struct HeapProfilerState {
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

    // Merge child process profile results into parent process.
    //
    // Usually this should be called when child process exits.
    pub fn merge(&mut self, profiler: HeapProfilerState) {
        self.started = std::cmp::min(self.started, profiler.started);
        let started_delta = profiler.started.duration_since(self.started).unwrap();
        // TODO sorted state merging of two sorted lists can be done in O(N+M)
        // currently this runs in O((N+M)*log(N+M)*C)
        profiler.heap_history.iter().for_each(|(h, d)| {
            self.heap_history.push_back((*h, *d + started_delta));
        });
        self.heap_history
            .make_contiguous()
            .sort_unstable_by_key(|&(_, d)| d);
        // remove oldest extra elements (ringbuffer)
        if self.heap_history.len() > HISTORY_CAPACITY {
            self.heap_history
                .drain(0..(self.heap_history.len() - HISTORY_CAPACITY));
        }
    }

    // Deallocate remaining memory. This is invoked when process exits
    pub fn free_all(&mut self) {
        // implicitly deallocate all left over child memory
        self.memory.clone().iter().for_each(|(k, _)| {
            self.free_profiler(*k);
        });
    }

    // Write heap profile history to a file. Format:
    //
    // #time/sec heap/byte
    // 0.1       10
    // 0.2       15
    // ...
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

    fn history_push(&mut self, change: i32) {
        if self.heap_history.len() == HISTORY_CAPACITY {
            // if HISTRY_CAPACITY > 0 this should be safe
            self.heap_history.pop_front().unwrap();
        }
        // TODO: trap/log error if elapsed failed
        // very unlikely to happen!
        self.heap_history
            .push_back((change, self.started.elapsed().unwrap()));
    }
}

#[host_functions(namespace = "heap_profiler", sync = "mutex")]
impl HeapProfilerState {
    fn aligned_alloc_profiler(&mut self, _self: u32, size: Size, ptr: Ptr) {
        self.malloc_profiler(size, ptr);
    }

    fn malloc_profiler(&mut self, size: Size, ptr: Ptr) {
        debug!("heap_profiler: malloc({}) -> {}", size, ptr);
        self.memory.insert(ptr, size);
        self.history_push(size as i32);
    }

    fn calloc_profiler(&mut self, len: Size, elem_size: Size, ptr: Ptr) {
        debug!("heap_profiler: calloc({},{}) -> {}", len, elem_size, ptr);
        let size = len * elem_size;
        self.memory.insert(ptr, size);
        self.history_push(size as i32);
    }

    fn realloc_profiler(&mut self, old_ptr: Ptr, size: Size, new_ptr: Ptr) {
        debug!(
            "heap_profiler: realloc({},{}) -> {}",
            old_ptr, size, new_ptr
        );
        match self.memory.remove(&old_ptr) {
            Some(removed_size) => {
                self.memory.insert(new_ptr, size);
                let size_delta = size as i32 - removed_size as i32;
                self.history_push(size_delta);
            }
            None => error!(
                "heap_profiler: can't reallocate, pointer {} doesn't exist",
                old_ptr
            ),
        };
    }

    fn free_profiler(&mut self, ptr: Ptr) {
        debug!("heap_profiler: free({})", ptr);
        if ptr != 0 {
            match self.memory.remove(&ptr) {
                Some(size) => {
                    self.history_push(-(size as i32));
                }
                None => error!("heap_profiler: can't free, pointer {} doesn't exist", ptr),
            };
        }
    }
}
