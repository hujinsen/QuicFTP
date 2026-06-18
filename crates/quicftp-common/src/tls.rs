use rcgen::{CertificateParams, KeyPair};
use std::path::Path;
use tracing::info;

/// Generate a self-signed certificate and save to files
pub fn generate_self_signed_cert(cert_path: &Path, key_path: &Path) -> anyhow::Result<()> {
    info!("Generating self-signed certificate...");

    let key_pair = KeyPair::generate()?;

    let mut params = CertificateParams::new(vec!["localhost".to_string()])?;
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        rcgen::DnValue::Utf8String("QuicFTP Server".to_string()),
    );

    let cert = params.self_signed(&key_pair)?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    if let Some(parent) = cert_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Some(parent) = key_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    std::fs::write(cert_path, &cert_pem)?;
    std::fs::write(key_path, &key_pem)?;

    info!("Certificate saved to: {:?}", cert_path);
    info!("Private key saved to: {:?}", key_path);

    Ok(())
}

/// Load certificate and key from files
pub fn load_cert_and_key(cert_path: &Path, key_path: &Path) -> anyhow::Result<(Vec<u8>, Vec<u8>)> {
    let cert_pem = std::fs::read(cert_path)?;
    let key_pem = std::fs::read(key_path)?;
    Ok((cert_pem, key_pem))
}

/// Create a rustls server config from PEM files
pub fn create_server_tls_config(
    cert_path: &Path,
    key_path: &Path,
) -> anyhow::Result<rustls::ServerConfig> {
    let (cert_pem, key_pem) = load_cert_and_key(cert_path, key_path)?;

    let certs = rustls_pemfile::certs(&mut &cert_pem[..]).collect::<Result<Vec<_>, _>>()?;

    let key = rustls_pemfile::private_key(&mut &key_pem[..])?
        .ok_or_else(|| anyhow::anyhow!("No private key found"))?;

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(config)
}

/// Create a rustls client config that accepts self-signed certificates
pub fn create_client_tls_config() -> anyhow::Result<rustls::ClientConfig> {
    use rustls::client::danger::{
        HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier,
    };
    use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
    use rustls::{DigitallySignedStruct, Error, SignatureScheme};

    #[derive(Debug)]
    struct NoVerifier;

    impl ServerCertVerifier for NoVerifier {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dcs: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn verify_tls13_signature(
            &self,
            _message: &[u8],
            _cert: &CertificateDer<'_>,
            _dcs: &DigitallySignedStruct,
        ) -> Result<HandshakeSignatureValid, Error> {
            Ok(HandshakeSignatureValid::assertion())
        }

        fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
            vec![
                SignatureScheme::RSA_PKCS1_SHA256,
                SignatureScheme::RSA_PKCS1_SHA384,
                SignatureScheme::RSA_PKCS1_SHA512,
                SignatureScheme::ECDSA_NISTP256_SHA256,
                SignatureScheme::ECDSA_NISTP384_SHA384,
                SignatureScheme::ECDSA_NISTP521_SHA512,
                SignatureScheme::ED25519,
                SignatureScheme::RSA_PSS_SHA256,
                SignatureScheme::RSA_PSS_SHA384,
                SignatureScheme::RSA_PSS_SHA512,
            ]
        }
    }

    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(std::sync::Arc::new(NoVerifier))
        .with_no_client_auth();

    Ok(config)
}
