//! Native Windows verification for Microsoft's signed JSON manifests.
//!
//! Supplied certificates are supporting material, never trust anchors. Windows
//! must build a valid code-signing chain to its trusted root store, and the
//! resulting root must match the pinned Microsoft Root CA 2011 certificate.

use std::ffi::{c_void, CStr};
use std::mem::size_of;
use std::ptr::{null, null_mut};

use sha2::{Digest, Sha256};
use windows_sys::core::PCSTR;
use windows_sys::Win32::Security::Cryptography::{
    szOID_COMMON_NAME, szOID_ORGANIZATION_NAME, szOID_PKIX_KP_CODE_SIGNING, BCryptDestroyKey,
    BCryptVerifySignature, CertAddCertificateContextToStore, CertCloseStore,
    CertCreateCertificateContext, CertFreeCertificateChain, CertFreeCertificateContext,
    CertGetCertificateChain, CertGetEnhancedKeyUsage, CertGetNameStringW, CertOpenStore,
    CertVerifyCertificateChainPolicy, CryptImportPublicKeyInfoEx2, BCRYPT_KEY_HANDLE,
    BCRYPT_PAD_PKCS1, BCRYPT_PKCS1_PADDING_INFO, BCRYPT_SHA256_ALGORITHM, CERT_CHAIN_CONTEXT,
    CERT_CHAIN_PARA, CERT_CHAIN_POLICY_AUTHENTICODE, CERT_CHAIN_POLICY_PARA,
    CERT_CHAIN_POLICY_STATUS, CERT_CHAIN_REVOCATION_ACCUMULATIVE_TIMEOUT,
    CERT_CHAIN_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, CERT_CONTEXT,
    CERT_FIND_EXT_ONLY_ENHKEY_USAGE_FLAG, CERT_NAME_ATTR_TYPE, CERT_STORE_ADD_USE_EXISTING,
    CERT_STORE_PROV_MEMORY, CERT_USAGE_MATCH, CTL_USAGE, HCERTSTORE, USAGE_MATCH_TYPE_AND,
    X509_ASN_ENCODING,
};

use crate::error::{MsvcKitError, Result};

const MICROSOFT_ROOT_CA_2011_SHA256: &str =
    "847df6a78497943f27fc72eb93f9a637320a02b561d0a91b09e87a7807ed7c61";
const MICROSOFT_PUBLISHER: &str = "Microsoft Corporation";
const REVOCATION_TIMEOUT_MS: u32 = 10_000;

struct Certificate(*mut CERT_CONTEXT);

impl Certificate {
    fn decode(encoded: &[u8]) -> Result<Self> {
        let length = u32::try_from(encoded.len())
            .map_err(|_| verification_error("certificate is too large"))?;
        if encoded.is_empty() {
            return Err(verification_error("certificate is empty"));
        }
        // SAFETY: The input slice remains valid for the call. Windows copies the
        // encoded bytes into the returned context, which this owner releases.
        let context =
            unsafe { CertCreateCertificateContext(X509_ASN_ENCODING, encoded.as_ptr(), length) };
        if context.is_null() {
            return Err(windows_error("decoding certificate"));
        }
        Ok(Self(context))
    }
}

impl Drop for Certificate {
    fn drop(&mut self) {
        // SAFETY: This is the non-null context returned by CertCreateCertificateContext.
        unsafe { CertFreeCertificateContext(self.0) };
    }
}

struct CertificateStore(HCERTSTORE);

impl CertificateStore {
    fn memory() -> Result<Self> {
        // SAFETY: The memory provider needs no parameters or external resources.
        let store = unsafe { CertOpenStore(CERT_STORE_PROV_MEMORY, 0, 0, 0, null()) };
        if store.is_null() {
            return Err(windows_error("opening intermediate certificate store"));
        }
        Ok(Self(store))
    }

    fn add(&self, certificate: &Certificate) -> Result<()> {
        // SAFETY: Both handles are live. Windows copies the context into this
        // supporting store; no output context is requested or leaked.
        let success = unsafe {
            CertAddCertificateContextToStore(
                self.0,
                certificate.0,
                CERT_STORE_ADD_USE_EXISTING,
                null_mut(),
            )
        };
        if success == 0 {
            return Err(windows_error("adding intermediate certificate"));
        }
        Ok(())
    }
}

impl Drop for CertificateStore {
    fn drop(&mut self) {
        // SAFETY: This owner holds a live memory store, and all dependent chains
        // have been dropped before the store is released.
        unsafe { CertCloseStore(self.0, 0) };
    }
}

struct CertificateChain(*mut CERT_CHAIN_CONTEXT);

impl Drop for CertificateChain {
    fn drop(&mut self) {
        // SAFETY: This is the non-null chain returned by CertGetCertificateChain.
        unsafe { CertFreeCertificateChain(self.0) };
    }
}

struct PublicKey(BCRYPT_KEY_HANDLE);

impl Drop for PublicKey {
    fn drop(&mut self) {
        // SAFETY: This owner holds the non-null CNG key imported below.
        unsafe { BCryptDestroyKey(self.0) };
    }
}

/// Verify RSA PKCS#1/SHA-256 over the caller's exact signed-info bytes.
///
/// The first certificate must be the signer; remaining certificates only help
/// Windows construct its chain. Expiry, revocation, usage and trust failures
/// fail closed, including unavailable revocation information.
pub(super) fn verify_signature(
    certificates: &[Vec<u8>],
    signature: &[u8],
    signed_info: &[u8],
) -> Result<()> {
    let signer = certificates
        .first()
        .ok_or_else(|| verification_error("manifest has no signer certificate"))?;
    if signature.is_empty() || signed_info.is_empty() {
        return Err(verification_error(
            "signature or signed information is empty",
        ));
    }
    let store = CertificateStore::memory()?;
    let leaf = Certificate::decode(signer)?;
    for encoded in certificates.iter().skip(1) {
        store.add(&Certificate::decode(encoded)?)?;
    }
    verify_publisher(&leaf)?;
    require_code_signing_usage(&leaf)?;
    let chain = build_chain(&leaf, &store)?;
    verify_chain_policy(&chain)?;
    // SAFETY: This live Windows chain owns every nested certificate pointer
    // throughout root-pin verification.
    unsafe { verify_root_pin(&*chain.0) }?;

    verify_rsa(signer, signature, signed_info)
}

/// Check the cryptographic signature only, without asserting signer trust.
///
/// Kept separate so deterministic fixture tests can validate signed-byte
/// normalization without relying on online revocation or certificate expiry.
pub(super) fn verify_rsa(certificate: &[u8], signature: &[u8], signed_info: &[u8]) -> Result<()> {
    if signature.is_empty() || signed_info.is_empty() {
        return Err(verification_error(
            "signature or signed information is empty",
        ));
    }
    let signature_length =
        u32::try_from(signature.len()).map_err(|_| verification_error("signature is too large"))?;
    let leaf = Certificate::decode(certificate)?;

    // SAFETY: The certificate context owns its decoded CERT_INFO for this scope.
    let certificate_info = unsafe { (*leaf.0).pCertInfo.as_ref() }
        .ok_or_else(|| verification_error("signer has no public-key information"))?;
    let mut key_handle = null_mut();
    // SAFETY: The decoded public-key info is live, the reserved parameter is
    // null, and the output points to writable storage for a CNG key handle.
    let imported = unsafe {
        CryptImportPublicKeyInfoEx2(
            X509_ASN_ENCODING,
            &certificate_info.SubjectPublicKeyInfo,
            0,
            null(),
            &mut key_handle,
        )
    };
    if imported == 0 || key_handle.is_null() {
        return Err(windows_error("importing signer public key"));
    }
    let key = PublicKey(key_handle);
    let digest = Sha256::digest(signed_info);
    let padding = BCRYPT_PKCS1_PADDING_INFO {
        pszAlgId: BCRYPT_SHA256_ALGORITHM,
    };
    // SAFETY: The live imported key and all buffers remain valid throughout the
    // call. The padding structure matches BCRYPT_PAD_PKCS1 and SHA-256.
    let status = unsafe {
        BCryptVerifySignature(
            key.0,
            (&padding as *const BCRYPT_PKCS1_PADDING_INFO).cast(),
            digest.as_ptr(),
            digest.len() as u32,
            signature.as_ptr(),
            signature_length,
            BCRYPT_PAD_PKCS1,
        )
    };
    if status != 0 {
        return Err(verification_error(format!(
            "RSA/SHA-256 signature is invalid (NTSTATUS {:#010x})",
            status as u32
        )));
    }
    Ok(())
}

fn verify_publisher(certificate: &Certificate) -> Result<()> {
    for oid in [szOID_COMMON_NAME, szOID_ORGANIZATION_NAME] {
        if subject_attribute(certificate, oid)? != MICROSOFT_PUBLISHER {
            return Err(verification_error(
                "signer subject CN and O must both be Microsoft Corporation",
            ));
        }
    }
    Ok(())
}

fn subject_attribute(certificate: &Certificate, oid: PCSTR) -> Result<String> {
    // SAFETY: The live certificate and static, terminated OID are valid. A null
    // output asks Windows for the complete UTF-16 size, including terminator.
    let length = unsafe {
        CertGetNameStringW(
            certificate.0,
            CERT_NAME_ATTR_TYPE,
            0,
            oid.cast::<c_void>(),
            null_mut(),
            0,
        )
    };
    if length <= 1 {
        return Err(verification_error("signer subject attribute is missing"));
    }
    let mut name = vec![0u16; length as usize];
    // SAFETY: The output vector contains exactly the number of UTF-16 elements
    // Windows requested, and all other inputs are unchanged and live.
    let written = unsafe {
        CertGetNameStringW(
            certificate.0,
            CERT_NAME_ATTR_TYPE,
            0,
            oid.cast::<c_void>(),
            name.as_mut_ptr(),
            length,
        )
    };
    if written != length || name.last() != Some(&0) || name[..name.len() - 1].contains(&0) {
        return Err(verification_error("signer subject attribute is malformed"));
    }
    String::from_utf16(&name[..name.len() - 1])
        .map_err(|_| verification_error("signer subject attribute is invalid UTF-16"))
}

fn require_code_signing_usage(certificate: &Certificate) -> Result<()> {
    let mut length = 0;
    // SAFETY: The live certificate is valid; a null output obtains the decoded
    // extension size without treating mutable store properties as an EKU.
    let success = unsafe {
        CertGetEnhancedKeyUsage(
            certificate.0,
            CERT_FIND_EXT_ONLY_ENHKEY_USAGE_FLAG,
            null_mut(),
            &mut length,
        )
    };
    if success == 0 || (length as usize) < size_of::<CTL_USAGE>() {
        return Err(verification_error(
            "signer has no code-signing EKU extension",
        ));
    }
    // A usize allocation provides sufficient alignment for CTL_USAGE and its
    // pointer array, unlike a byte vector.
    let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
    let usage = buffer.as_mut_ptr().cast::<CTL_USAGE>();
    // SAFETY: The aligned output covers the requested byte count. Windows fills
    // the structure and its nested pointer array within that same allocation.
    if unsafe {
        CertGetEnhancedKeyUsage(
            certificate.0,
            CERT_FIND_EXT_ONLY_ENHKEY_USAGE_FLAG,
            usage,
            &mut length,
        )
    } == 0
    {
        return Err(windows_error("reading signer code-signing EKU"));
    }
    // SAFETY: The successful API call initialized this aligned structure and
    // any identifiers it returns remain valid while buffer is alive.
    let usage = unsafe { &*usage };
    if usage.cUsageIdentifier == 0 || usage.rgpszUsageIdentifier.is_null() {
        return Err(verification_error("signer has no code-signing EKU"));
    }
    for index in 0..usage.cUsageIdentifier as usize {
        // SAFETY: Windows owns the shape of this decoded pointer array and its
        // null-terminated OID strings inside the live output allocation.
        let oid = unsafe { *usage.rgpszUsageIdentifier.add(index) };
        if !oid.is_null()
            && unsafe { CStr::from_ptr(oid.cast()) }.to_bytes() == b"1.3.6.1.5.5.7.3.3"
        {
            return Ok(());
        }
    }
    Err(verification_error(
        "signer EKU does not permit code signing",
    ))
}

fn build_chain(certificate: &Certificate, store: &CertificateStore) -> Result<CertificateChain> {
    let mut usage_oid = szOID_PKIX_KP_CODE_SIGNING.cast_mut();
    let parameters = CERT_CHAIN_PARA {
        cbSize: size_of::<CERT_CHAIN_PARA>() as u32,
        RequestedUsage: CERT_USAGE_MATCH {
            dwType: USAGE_MATCH_TYPE_AND,
            Usage: CTL_USAGE {
                cUsageIdentifier: 1,
                rgpszUsageIdentifier: &mut usage_oid,
            },
        },
        dwUrlRetrievalTimeout: REVOCATION_TIMEOUT_MS,
        ..Default::default()
    };
    let mut context = null_mut();
    // SAFETY: All inputs remain live throughout this synchronous call. Null
    // engine selects Windows trust, null time checks now, and no cache-only or
    // ignore flags weaken online revocation checking or certificate validity.
    let success = unsafe {
        CertGetCertificateChain(
            null_mut(),
            certificate.0,
            null(),
            store.0,
            &parameters,
            CERT_CHAIN_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT
                | CERT_CHAIN_REVOCATION_ACCUMULATIVE_TIMEOUT,
            null(),
            &mut context,
        )
    };
    if success == 0 || context.is_null() {
        return Err(windows_error(
            "building online code-signing certificate chain",
        ));
    }
    let chain = CertificateChain(context);
    // SAFETY: The chain is a live, successful API result owned by this scope.
    let errors = unsafe { (*chain.0).TrustStatus.dwErrorStatus };
    if errors != 0 {
        return Err(verification_error(format!(
            "certificate chain trust failed (status {errors:#010x}); expiry, revocation and unavailable revocation information are rejected"
        )));
    }
    Ok(chain)
}

fn verify_chain_policy(chain: &CertificateChain) -> Result<()> {
    let parameters = CERT_CHAIN_POLICY_PARA {
        cbSize: size_of::<CERT_CHAIN_POLICY_PARA>() as u32,
        ..Default::default()
    };
    let mut status = CERT_CHAIN_POLICY_STATUS {
        cbSize: size_of::<CERT_CHAIN_POLICY_STATUS>() as u32,
        ..Default::default()
    };
    // SAFETY: The chain is live and all policy structures have correct sizes.
    // Zero policy flags preserve all Authenticode trust checks.
    let success = unsafe {
        CertVerifyCertificateChainPolicy(
            CERT_CHAIN_POLICY_AUTHENTICODE,
            chain.0,
            &parameters,
            &mut status,
        )
    };
    if success == 0 {
        return Err(windows_error("checking Authenticode certificate policy"));
    }
    if status.dwError != 0 {
        return Err(verification_error(format!(
            "Authenticode certificate policy failed (status {:#010x})",
            status.dwError
        )));
    }
    Ok(())
}

/// # Safety
/// Every non-null nested pointer must address a valid chain element or
/// certificate for the duration of the call. Production supplies an OS-owned
/// chain; tests may use live stack structures to exercise pinning independently
/// of Windows trust decisions.
unsafe fn verify_root_pin(chain: &CERT_CHAIN_CONTEXT) -> Result<()> {
    // SAFETY: The caller guarantees nested pointer validity. Counts and null
    // checks precede indexing and constructing the DER slice.
    let root = unsafe {
        if chain.cChain == 0 || chain.rgpChain.is_null() {
            return Err(verification_error("certificate chain has no simple chain"));
        }
        let simple_chain = (*chain.rgpChain)
            .as_ref()
            .ok_or_else(|| verification_error("certificate chain is incomplete"))?;
        if simple_chain.cElement == 0 || simple_chain.rgpElement.is_null() {
            return Err(verification_error("certificate chain has no root element"));
        }
        let root_element = (*simple_chain
            .rgpElement
            .add(simple_chain.cElement as usize - 1))
        .as_ref()
        .ok_or_else(|| verification_error("root chain element is absent"))?;
        root_element
            .pCertContext
            .as_ref()
            .ok_or_else(|| verification_error("root certificate is absent"))?
    };
    if root.pbCertEncoded.is_null() || root.cbCertEncoded == 0 {
        return Err(verification_error("root certificate has no encoded bytes"));
    }
    // SAFETY: Windows exposes this nonempty DER buffer with its exact size, and
    // the owning chain remains alive throughout hashing.
    let encoded =
        unsafe { std::slice::from_raw_parts(root.pbCertEncoded, root.cbCertEncoded as usize) };
    if hex::encode(Sha256::digest(encoded)) != MICROSOFT_ROOT_CA_2011_SHA256 {
        return Err(verification_error(
            "certificate chain does not terminate at pinned Microsoft Root CA 2011",
        ));
    }
    Ok(())
}

fn windows_error(operation: &str) -> MsvcKitError {
    verification_error(format!("{operation}: {}", std::io::Error::last_os_error()))
}

fn verification_error(message: impl std::fmt::Display) -> MsvcKitError {
    MsvcKitError::Other(format!(
        "Microsoft manifest signature verification failed: {message}"
    ))
}

#[cfg(test)]
mod tests {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use windows_sys::Win32::Security::Cryptography::{
        CERT_CHAIN_CONTEXT, CERT_CHAIN_ELEMENT, CERT_CONTEXT, CERT_SIMPLE_CHAIN,
    };

    use super::{
        require_code_signing_usage, verify_publisher, verify_root_pin, verify_rsa,
        verify_signature, Certificate, Result,
    };

    // Locally generated RSA certificate with Microsoft subject text and a
    // code-signing EKU. It was never installed into a trust store; its private
    // key was discarded. Subject strings alone must not authorize a manifest.
    const FORGED_MICROSOFT_CERTIFICATE: &str = concat!(
        "3082032d30820215a003020102020900a1a6ada96381a26f300d06092a864886f70d01010b05003040311e301c060355040a13154d6963726f736f667420436f72706f726174696f6e",
        "311e301c060355040313154d6963726f736f667420436f72706f726174696f6e301e170d3236303130313030303030305a170d3336303130313030303030305a3040311e301c060355",
        "040a13154d6963726f736f667420436f72706f726174696f6e311e301c060355040313154d6963726f736f667420436f72706f726174696f6e30820122300d06092a864886f70d0101",
        "0105000382010f003082010a0282010100ec2b44323658c11d6b93a23f6f59d29e0c30444b597cf0c79dcef2bbe9c7b5ebc63d7dd489e27c4f02f9b74d140cc0c6b4bb5c85a062",
        "f8bedef6d23f56e6c6a6eb8e4cb3a4586bd00593717e5b5884bcfa102bf069f49909689c1b856e72de341b0fde898daa35809158c9c847556ea976e8fc4c786ae963c45b7db898d",
        "f44d8823f10b345003413e901acd2ffba4c280ce7b4e512fc16fd45359297d21d96eb8ee8dcce47c69f6883bb21b24f35ac2df3ee93f8debf7a072b14e532f7a62fe4752a33d3f",
        "4937912534a703b2ee0234cfbb2a48d370e3a10b49f619af5bd5d1db6f6edfb97ea788b62b5682a66517df6f379367784d89b1a104f2a48a4cb0fb50203010001a32a3028301606",
        "03551d250101ff040c300a06082b06010505070303300e0603551d0f0101ff040403020780300d06092a864886f70d01010b050003820101000c54da399d7066108a22c79ad5286c1d",
        "8a06c6a8b63f180aff33e988a13e4d735fbc75ba564e200fd82b68cc26b1d424ef9ffb55a7a9bc5c72cc9d5da789716b6dbb706397ae574029c4e9064bb8770b3468fa6dcfce2b",
        "6c6c80d47824fd4d0bc42c3f4e778cdef13c6c0c65ed16f8fb130f512f1f862db2640dbce06476b4bac124e02d4a7234106f3c4b1c403dd8ddb8d737c68881d8a2e7d2e12911e",
        "b413933defee44ddf29d4949e1a6f86bc23f2748871d5d6082da9fd96606bf50c334f15e0225aefd5ee033a0f98ba9cdd9bcbe4e198b837120134cdfc8ece1f1eb5961dc67de1143",
        "d64958bccf9f503135d76a84ba2899b42cb2964d15b9471b2a75d",
    );

    fn official_certificates() -> Vec<Vec<u8>> {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/visualstudio-17-release.channel.json"
        ))
        .unwrap();
        fixture["signature"]["keyInfo"]["x509Data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|certificate| STANDARD.decode(certificate.as_str().unwrap()).unwrap())
            .collect()
    }

    fn check_root_pin(context: Option<&CERT_CONTEXT>) -> Result<()> {
        let mut element = CERT_CHAIN_ELEMENT {
            pCertContext: context.map_or(std::ptr::null(), |context| context),
            ..Default::default()
        };
        let mut element_pointer = &mut element as *mut CERT_CHAIN_ELEMENT;
        let mut simple_chain = CERT_SIMPLE_CHAIN {
            cElement: 1,
            rgpElement: &mut element_pointer,
            ..Default::default()
        };
        let mut simple_chain_pointer = &mut simple_chain as *mut CERT_SIMPLE_CHAIN;
        let chain = CERT_CHAIN_CONTEXT {
            cChain: 1,
            rgpChain: &mut simple_chain_pointer,
            ..Default::default()
        };
        // SAFETY: All chain nodes are live stack values. A supplied certificate
        // context is borrowed for this call and its encoded buffer is live.
        unsafe { verify_root_pin(&chain) }
    }

    #[test]
    fn rejects_absent_signer_and_empty_signature_inputs() {
        assert!(verify_signature(&[], b"signature", b"signed info")
            .unwrap_err()
            .to_string()
            .contains("no signer certificate"));
        let certificates = vec![vec![0x30, 0x00]];
        for (signature, signed_info) in [
            (b"".as_slice(), b"signed info".as_slice()),
            (b"signature".as_slice(), b"".as_slice()),
        ] {
            assert!(verify_signature(&certificates, signature, signed_info)
                .unwrap_err()
                .to_string()
                .contains("signature or signed information is empty"));
            assert!(verify_rsa(&certificates[0], signature, signed_info)
                .unwrap_err()
                .to_string()
                .contains("signature or signed information is empty"));
        }
    }

    #[test]
    fn rejects_empty_or_malformed_certificates_before_chain_retrieval() {
        for certificate in [vec![], vec![0x30, 0x00], b"not a certificate".to_vec()] {
            let crypto_error = verify_rsa(&certificate, b"signature", b"signed info")
                .unwrap_err()
                .to_string();
            let trust_error = verify_signature(&[certificate], b"signature", b"signed info")
                .unwrap_err()
                .to_string();
            assert!(crypto_error.contains("certificate"));
            assert!(trust_error.contains("certificate"));
        }
    }

    #[test]
    fn rejects_self_signed_root_even_with_microsoft_publisher_and_code_signing_eku() {
        let encoded = hex::decode(FORGED_MICROSOFT_CERTIFICATE).unwrap();
        let certificate = Certificate::decode(&encoded).unwrap();
        verify_publisher(&certificate).unwrap();
        require_code_signing_usage(&certificate).unwrap();
        let error = verify_signature(&[encoded], b"signature", b"signed info")
            .unwrap_err()
            .to_string();
        assert!(error.contains("certificate chain trust failed"), "{error}");
    }

    #[test]
    fn pins_official_root_and_rejects_forged_or_absent_roots() {
        let certificates = official_certificates();
        let root = Certificate::decode(certificates.last().unwrap()).unwrap();
        // SAFETY: These owners keep their decoded contexts and DER alive.
        check_root_pin(Some(unsafe { &*root.0 })).unwrap();
        let forged =
            Certificate::decode(&hex::decode(FORGED_MICROSOFT_CERTIFICATE).unwrap()).unwrap();
        assert!(check_root_pin(Some(unsafe { &*forged.0 })).is_err());
        assert!(check_root_pin(None).is_err());
        assert!(check_root_pin(Some(&CERT_CONTEXT::default())).is_err());
        // SAFETY: An empty chain contains no pointers that can be dereferenced.
        assert!(unsafe { verify_root_pin(&CERT_CHAIN_CONTEXT::default()) }.is_err());
    }

    #[test]
    fn rejects_issuer_certificates_as_publishers_and_missing_leaf_usage() {
        let certificates = official_certificates();
        let leaf = Certificate::decode(&certificates[0]).unwrap();
        verify_publisher(&leaf).unwrap();
        require_code_signing_usage(&leaf).unwrap();
        for encoded in certificates.iter().skip(1) {
            let issuer = Certificate::decode(encoded).unwrap();
            assert!(verify_publisher(&issuer).is_err());
        }
        let root = Certificate::decode(certificates.last().unwrap()).unwrap();
        assert!(require_code_signing_usage(&root).is_err());
    }
}
