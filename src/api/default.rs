use uptown_funk::{Executor, HostFunctions};

use crate::api::channel::ChannelReceiver;
use crate::module::LunaticModule;

use crate::api::{channel, networking, process};

use super::wasi::state::WasiState;
pub struct DefaultApi {
    context_receiver: Option<ChannelReceiver>,
    module: LunaticModule,
    wasi_ret: Option<<WasiState as HostFunctions>::Return>,
    wasi_wrap: Option<<WasiState as HostFunctions>::Wrap>,
}

impl DefaultApi {
    pub fn new(context_receiver: Option<ChannelReceiver>, module: LunaticModule) -> Self {
        let (wasi_ret, wasi_wrap) = WasiState::new().split();
        Self {
            context_receiver,
            module,
            wasi_ret: Some(wasi_ret),
            wasi_wrap: Some(wasi_wrap),
        }
    }
}

impl HostFunctions for DefaultApi {
    type Return = <WasiState as HostFunctions>::Wrap;
    type Wrap = Self;

    fn split(mut self) -> (Self::Return, Self::Wrap) {
        (self.wasi_ret.take().unwrap(), self)
    }

    fn add_to_linker<E>(mut api: Self, executor: E, linker: &mut wasmtime::Linker)
    where
        E: Executor + Clone + 'static,
    {
        let channel_state = channel::api::ChannelState::new(api.context_receiver);
        let (_, channel_state) = channel_state.split();
        channel::api::ChannelState::add_to_linker(channel_state.clone(), executor.clone(), linker);

        let process_state = process::api::ProcessState::new(api.module, channel_state.clone());
        let (_, process_state) = process_state.split();
        process::api::ProcessState::add_to_linker(process_state, executor.clone(), linker);

        let networking_state = networking::TcpState::new(channel_state);
        let (_, networking_state) = networking_state.split();
        networking::TcpState::add_to_linker(networking_state, executor.clone(), linker);

        WasiState::add_to_linker(api.wasi_wrap.take().unwrap(), executor, linker);
    }
}
