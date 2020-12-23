use std::io::IoSliceMut;

#[allow(improper_ctypes)]
#[link(wasm_import_module = "env")]
extern "C" {
    fn vectored_read(ptr: *mut IoSliceMut<'_>, len: usize);
}

#[export_name = "test_mut_ioslice"]
pub extern "C" fn test_mut_ioslice() {
    let mut buf0 = [0; 8];
    let mut buf1 = [0; 8];
    let mut buf2 = [0; 8];
    let mut buf3 = [0; 8];
    let bufs = &mut [
        IoSliceMut::new(&mut buf0),
        IoSliceMut::new(&mut buf1),
        IoSliceMut::new(&mut buf2),
        IoSliceMut::new(&mut buf3),
    ];

    unsafe { vectored_read(bufs.as_mut_ptr(), bufs.len()) };
    assert_eq!(buf0, [0; 8]);
    assert_eq!(buf1, [1; 8]);
    assert_eq!(buf2, [2; 8]);
    assert_eq!(buf3, [3; 8]);
}
