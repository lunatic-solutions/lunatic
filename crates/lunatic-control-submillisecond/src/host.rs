use serde::{Deserialize, Serialize};

mod api {
    #[link(wasm_import_module = "lunatic::distributed")]
    extern "C" {
        pub fn test_root_cert(len_ptr: *mut u32) -> u32;
        pub fn default_server_certificates(
            cert_pem_ptr: *const u8,
            cert_pem_len: u32,
            key_pair_pem_ptr: *const u8,
            key_pair_pem_len: u32,
            len_ptr: *mut u32,
        ) -> u32;
        pub fn sign_node(
            cert_pem_ptr: *const u8,
            cert_pem_len: u32,
            key_pair_pem_ptr: *const u8,
            key_pair_pem_len: u32,
            csr_pem_ptr: *const u8,
            csr_pem_len: u32,
            len_ptr: *mut u32,
        ) -> u32;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertPk {
    pub cert: String,
    pub pk: String,
}

pub fn test_root_cert() -> CertPk {
    let (cert, pk) = call_host_alloc(|len_ptr| unsafe { api::test_root_cert(len_ptr) }).unwrap();
    CertPk { cert, pk }
}

pub fn default_server_certificates(cert_pem: &str, pk_pem: &str) -> CertPk {
    let (cert, pk) = call_host_alloc(|len_ptr| unsafe {
        api::default_server_certificates(
            cert_pem.as_ptr(),
            cert_pem.len() as u32,
            pk_pem.as_ptr(),
            pk_pem.len() as u32,
            len_ptr,
        )
    })
    .unwrap();
    CertPk { cert, pk }
}

pub fn sign_node(cert_pem: &str, pk_pem: &str, csr_pem: &str) -> String {
    call_host_alloc(|len_ptr| unsafe {
        api::sign_node(
            cert_pem.as_ptr(),
            cert_pem.len() as u32,
            pk_pem.as_ptr(),
            pk_pem.len() as u32,
            csr_pem.as_ptr(),
            csr_pem.len() as u32,
            len_ptr,
        )
    })
    .unwrap()
}

fn call_host_alloc<T>(f: impl Fn(*mut u32) -> u32) -> bincode::Result<T>
where
    T: for<'de> Deserialize<'de>,
{
    let mut len = 0_u32;
    let len_ptr = &mut len as *mut u32;
    let ptr = f(len_ptr);
    let data_vec = unsafe { Vec::from_raw_parts(ptr as *mut u8, len as usize, len as usize) };
    bincode::deserialize(&data_vec)
}
