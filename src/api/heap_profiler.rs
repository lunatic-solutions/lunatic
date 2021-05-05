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

    // Merge child process profile results into parent process profile.
    //
    // Usually this should be called when child process exits.
    //
    // It is assumed self is a parrent process profile and profiler is a child
    // process profile and thus self.started <= profiler.started is assumed.
    //
    // It is assumed HeapProfilerHistory.heap_history is sorted ascending by Duration.
    // This assumption will break if user messes with OS system time (if user reverts system clock
    // during profiling)
    pub fn merge(&mut self, mut profiler: HeapProfilerState) {
        let started_delta = profiler.started.duration_since(self.started).unwrap();

        let merged_size = std::cmp::min(
            self.heap_history.len() + profiler.heap_history.len(),
            HISTORY_CAPACITY,
        );
        let mut merged = VecDeque::with_capacity(merged_size);

        // Merge two sorted lists with respect to ringbuffer (keep at most HISTORY_CAPACITY bigger
        // elements)
        for _ in 0..merged_size {
            match (self.heap_history.back(), profiler.heap_history.back()) {
                (Some(_), None) => merged.push_front(self.heap_history.pop_back().unwrap()),
                (None, Some((h, d))) => {
                    merged.push_front((*h, *d + started_delta));
                    profiler.heap_history.pop_back();
                }
                (Some((_, d1)), Some((h2, d2))) => {
                    let d2_delta = *d2 + started_delta;
                    if d1 > &d2_delta {
                        merged.push_front(self.heap_history.pop_back().unwrap());
                    } else {
                        merged.push_front((*h2, d2_delta));
                        profiler.heap_history.pop_back();
                    }
                }
                // this line should never be triggered, we could panic instead
                (None, None) => break,
            }
        }
        self.heap_history = merged;
    }

    // Deallocate remaining memory. This is invoked when process exits
    pub fn free_all(&mut self) {
        // implicitly deallocate all left over child memory
        self.history_push(-(self.memory.values().sum::<u32>() as i32));
        self.memory.clear();
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
        match self.started.elapsed() {
            Ok(duration) => self.heap_history.push_back((change, duration)),
            Err(e) => error!("profiler_history_push: {}", e),
        }
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
        // realloc spec: if ptr is null then the call is equivalent  to malloc(size)
        if old_ptr == 0 {
            self.memory.insert(new_ptr, size);
            self.history_push(size as i32);
        } else {
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
    }

    fn free_profiler(&mut self, ptr: Ptr) {
        debug!("heap_profiler: free({})", ptr);
        // free spec: if ptr is null no action is performed
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

#[test]
fn merge_profiles() {
    // TODO use quickcheck crate
    fn random_memory() -> HashMap<Ptr, Size> {
        let len = rand::random::<usize>() % (HISTORY_CAPACITY + 1);
        let mut r = HashMap::with_capacity(len);
        for _ in 0..len {
            r.insert(rand::random(), rand::random());
        }
        r
    }
    fn random_history() -> VecDeque<(i32, Duration)> {
        let len = rand::random::<usize>() % (HISTORY_CAPACITY + 1);
        let mut r = VecDeque::with_capacity(len);
        for _ in 0..len {
            r.push_back((rand::random(), Duration::from_millis(rand::random())));
        }
        // we assume HeapProfilerHistory will keep heap_history sorted
        // this assumption will break if user messes with system time
        r.make_contiguous().sort_unstable_by_key(|&(_, d)| d);
        r
    }
    let mut parent = HeapProfilerState {
        memory: random_memory(),
        started: std::time::UNIX_EPOCH + Duration::from_millis(rand::random()),
        heap_history: random_history(),
    };
    let child = HeapProfilerState {
        memory: random_memory(),
        // we assume child is started after parent process
        started: parent.started + Duration::from_millis(rand::random()),
        heap_history: random_history(),
    };
    let mut parent_clone = HeapProfilerState {
        memory: parent.memory.clone(),
        started: parent.started.clone(),
        heap_history: parent.heap_history.clone(),
    };
    let child_clone = HeapProfilerState {
        memory: child.memory.clone(),
        started: child.started.clone(),
        heap_history: child.heap_history.clone(),
    };
    fn merge_simple(parent: &mut HeapProfilerState, child: HeapProfilerState) {
        let started_delta = child.started.duration_since(parent.started).unwrap();
        // merge profiles
        child.heap_history.iter().for_each(|(h, d)| {
            parent.heap_history.push_back((*h, *d + started_delta));
        });
        // sort profiles
        parent
            .heap_history
            .make_contiguous()
            .sort_unstable_by_key(|&(_, d)| d);
        // remove oldest extra elements (ringbuffer)
        if parent.heap_history.len() > HISTORY_CAPACITY {
            parent
                .heap_history
                .drain(0..(parent.heap_history.len() - HISTORY_CAPACITY));
        }
    }
    parent.merge(child);
    merge_simple(&mut parent_clone, child_clone);
    assert_eq!(parent.heap_history, parent_clone.heap_history);
}
