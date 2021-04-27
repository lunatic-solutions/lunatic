use uptown_funk::{Executor, HostFunctions};

use crate::api::channel::ChannelReceiver;
use crate::module::LunaticModule;

use crate::api::{channel, networking, process, wasi};
pub struct DefaultApi {
    context_receiver: Option<ChannelReceiver>,
    module: LunaticModule,
}

impl DefaultApi {
    pub fn new(context_receiver: Option<ChannelReceiver>, module: LunaticModule) -> Self {
        Self {
            context_receiver,
            module,
        }
    }
}

impl HostFunctions for DefaultApi {
    type Return = ();
    type Wrap = Self;

    fn split(self) -> (Self::Return, Self::Wrap) {
        ((), self)
    }

    #[cfg(feature = "vm-wasmtime")]
    fn add_to_linker<E>(api: Self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        //let channel_state = channel::api::ChannelState::new(self.context_receiver);
        //let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        //let networking_state = networking::TcpState::new(channel_state.clone());

        //channel_state.add_to_linker(executor.clone(), linker);
        //process_state.add_to_linker(executor.clone(), linker);
        //networking_state.add_to_linker(executor.clone(), linker);

        let (_, state) = wasi::api::WasiState::new().split();
        wasi::api::WasiState::add_to_linker(state, executor, linker);

    }

    #[cfg(feature = "vm-wasmer")]
    fn add_to_wasmer_linker<E>(
        self,
        executor: E,
        linker: &mut uptown_funk::wasmer::WasmerLinker,
        store: &wasmer::Store,
    ) -> ()
    where
        E: Executor + Clone + 'static,
    {
        //let channel_state = channel::api::ChannelState::new(self.context_receiver);
        //let process_state = process::api::ProcessState::new(self.module, channel_state.clone());
        //let networking_state = networking::TcpState::new(channel_state.clone());
        //let wasi_state = wasi::api::WasiState::new();

        //channel_state.add_to_wasmer_linker(executor.clone(), linker, store);
        //process_state.add_to_wasmer_linker(executor.clone(), linker, store);
        //networking_state.add_to_wasmer_linker(executor.clone(), linker, store);
        //wasi_state.add_to_wasmer_linker(executor, linker, store);
    }

}
