macro_rules! for_each_function_signature {
    ($mac:ident) => {
        $mac!(0);
        $mac!(1 A1);
        $mac!(2 A1 A2);
        $mac!(3 A1 A2 A3);
        $mac!(4 A1 A2 A3 A4);
        $mac!(5 A1 A2 A3 A4 A5);
        $mac!(6 A1 A2 A3 A4 A5 A6);
        $mac!(7 A1 A2 A3 A4 A5 A6 A7);
        $mac!(8 A1 A2 A3 A4 A5 A6 A7 A8);
        $mac!(9 A1 A2 A3 A4 A5 A6 A7 A8 A9);
        $mac!(10 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10);
        $mac!(11 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11);
        $mac!(12 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12);
        $mac!(13 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13);
        $mac!(14 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14);
        $mac!(15 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15);
        $mac!(16 A1 A2 A3 A4 A5 A6 A7 A8 A9 A10 A11 A12 A13 A14 A15 A16);
    };
}

macro_rules! generate_wrap_async_func {
    ($num:tt $($args:ident)*) => (paste::paste!{
        // Adds async function to linker if the namespace matches the allowed list.
        #[allow(dead_code)]
        pub(crate) fn [<link_async $num _if_match>]<T, $($args,)* R>(
            linker: &mut Linker<T>,
            namespace: &str,
            name: &str,
            func: impl for<'a> Fn(Caller<'a, T>, $($args),*) -> Box<dyn Future<Output = R> + Send + 'a> + Send + Sync + 'static,
            namespace_filter: &Vec<String>,
        ) -> Result<()>
        where
            $($args: WasmTy,)*
            R: WasmRet,
        {
            if namespace_matches_filter(namespace, name, namespace_filter) {
                linker.[<func_wrap $num _async>](namespace, name, func)?;
            }
            Ok(())
        }
    })
}
