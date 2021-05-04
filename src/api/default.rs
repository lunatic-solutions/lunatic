use uptown_funk::{Executor, HostFunctions};

use crate::api::channel::ChannelReceiver;
use crate::module::LunaticModule;

use crate::api::{channel, heap_profiler, networking, process, wasi};

pub struct DefaultApi {
    context_receiver: Option<ChannelReceiver>,
    module: LunaticModule,
    profiler: <heap_profiler::HeapProfilerState as HostFunctions>::Wrap,
}

impl DefaultApi {
    pub fn new(context_receiver: Option<ChannelReceiver>, module: LunaticModule) -> Self {
        let (_, profiler) = heap_profiler::HeapProfilerState::new().split();
        Self {
            context_receiver,
            module,
            profiler,
        }
    }
}

impl HostFunctions for DefaultApi {
    type Return = <heap_profiler::HeapProfilerState as HostFunctions>::Wrap;
    type Wrap = Self;

    fn split(self) -> (Self::Return, Self::Wrap) {
        (self.profiler.clone(), self)
    }

    fn add_to_linker<E>(api: Self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(api.context_receiver);
        let (_, channel_state) = channel_state.split();
        channel::api::ChannelState::add_to_linker(channel_state.clone(), executor.clone(), linker);

        let process_state = process::api::ProcessState::new(
            api.module,
            channel_state.clone(),
            api.profiler.clone(),
        );
        let (_, process_state) = process_state.split();
        process::api::ProcessState::add_to_linker(process_state, executor.clone(), linker);

        let networking_state = networking::TcpState::new(channel_state);
        let (_, networking_state) = networking_state.split();
        networking::TcpState::add_to_linker(networking_state, executor.clone(), linker);

        let wasi_state = wasi::state::WasiState::new();
        let (_, wasi_state) = wasi_state.split();
        wasi::state::WasiState::add_to_linker(wasi_state, executor.clone(), linker);

        heap_profiler::HeapProfilerState::add_to_linker(api.profiler, executor, linker);
    }
}
