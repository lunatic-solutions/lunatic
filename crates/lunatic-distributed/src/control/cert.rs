use std::path::Path;

use anyhow::Result;
use rcgen::*;

pub static TEST_ROOT_CERT: &str = r#"""
-----BEGIN CERTIFICATE-----
MIIBnDCCAUGgAwIBAgIIR5Hk+O5RdOgwCgYIKoZIzj0EAwIwKTEQMA4GA1UEAwwH
Um9vdCBDQTEVMBMGA1UECgwMTHVuYXRpYyBJbmMuMCAXDTc1MDEwMTAwMDAwMFoY
DzQwOTYwMTAxMDAwMDAwWjApMRAwDgYDVQQDDAdSb290IENBMRUwEwYDVQQKDAxM
dW5hdGljIEluYy4wWTATBgcqhkjOPQIBBggqhkjOPQMBBwNCAARlVNxYAwsmmFNc
2EMBbZZVwL8GBtnnu8IROdDd68ixc0VBjfrV0zAM344lKJcs9slsMTEofoYvMCpI
BhnSGyAFo1EwTzAdBgNVHREEFjAUghJyb290Lmx1bmF0aWMuY2xvdWQwHQYDVR0O
BBYEFOh0Ue745JFH76xErjqkW2/SbHhAMA8GA1UdEwEB/wQFMAMBAf8wCgYIKoZI
zj0EAwIDSQAwRgIhAJKPv4XUZ9ej+CVgsJ+9x/CmJEcnebyWh2KntJri97nxAiEA
/KvaQE6GtYZPGFv/WYM3YEmTQ7hoOvaaAuvD27cHkaw=
-----END CERTIFICATE-----
"""#;

pub static CTRL_SERVER_NAME: &str = "ctrl.lunatic.cloud";

static TEST_ROOT_KEYS: &str = r#"""
-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQg9ferf0du4h975Jhu
boMyGfdI+xwp7ewOulGvpTcvdpehRANCAARlVNxYAwsmmFNc2EMBbZZVwL8GBtnn
u8IROdDd68ixc0VBjfrV0zAM344lKJcs9slsMTEofoYvMCpIBhnSGyAF
-----END PRIVATE KEY-----"""#;

pub fn root_cert(
    test_ca: bool,
    ca_cert: Option<&str>,
    ca_keys: Option<&str>,
) -> Result<Certificate> {
    if test_ca {
        let key_pair = KeyPair::from_pem(TEST_ROOT_KEYS)?;
        let root_params = CertificateParams::from_ca_cert_pem(TEST_ROOT_CERT, key_pair)?;
        let root_cert = Certificate::from_params(root_params)?;
        Ok(root_cert)
    } else {
        let ca_cert_pem = std::fs::read(Path::new(
            ca_cert.ok_or_else(|| anyhow::anyhow!("Missing CA certificate."))?,
        ))?;
        let ca_keys_pem = std::fs::read(Path::new(
            ca_keys.ok_or_else(|| anyhow::anyhow!("Missing CA keys."))?,
        ))?;
        let key_pair = KeyPair::from_pem(std::str::from_utf8(&ca_keys_pem)?)?;
        let root_params =
            CertificateParams::from_ca_cert_pem(std::str::from_utf8(&ca_cert_pem)?, key_pair)?;
        let root_cert = Certificate::from_params(root_params)?;
        Ok(root_cert)
    }
}

//fn ctrl_cert() -> Result<Certificate> {
//    let mut ctrl_params = CertificateParams::new(vec![CTRL_SERVER_NAME.into()]);
//    ctrl_params
//        .distinguished_name
//        .push(DnType::OrganizationName, "Lunatic Inc.");
//    ctrl_params
//        .distinguished_name
//        .push(DnType::CommonName, "Control CA");
//    Ok(Certificate::from_params(ctrl_params)?)
//}
//
//fn default_server_certificates(root_cert: &Certificate) -> Result<(String, String)> {
//    let ctrl_cert = ctrl_cert()?;
//    let cert_pem = ctrl_cert.serialize_pem_with_signer(root_cert)?;
//    let key_pem = ctrl_cert.serialize_private_key_pem();
//    Ok((cert_pem, key_pem))
//}
