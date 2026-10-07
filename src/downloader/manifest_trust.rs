//! Authentication of Microsoft's signed JSON catalogs when channel metadata is stale.
//!
//! The catalog digest is still preferred. The alternative authenticates BOTH the
//! fresh channel and catalog, then binds their release identities. It never uses
//! an observed digest or mutable cache metadata as a trust anchor.

use crate::error::Result;

/// Other platforms continue to reject mismatching channel digests.
pub(super) fn verify_catalog(catalog: &[u8], channel: &[u8]) -> Result<()> {
    #[cfg(windows)]
    {
        signed::verify_catalog_with(
            catalog,
            channel,
            &super::manifest_trust_windows::verify_signature,
        )
    }
    #[cfg(not(windows))]
    {
        let _ = (catalog, channel);
        Err(crate::error::MsvcKitError::UnsupportedPlatform(
            "Microsoft signed-manifest verification requires Windows".into(),
        ))
    }
}

#[cfg(any(windows, test))]
mod signed {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use serde::de::{MapAccess, Visitor};
    use serde::{Deserialize, Deserializer};
    use serde_json::value::RawValue;
    use std::collections::HashSet;
    use std::fmt;

    use crate::error::{MsvcKitError, Result};

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Signature {
        sign_info: SignInfo,
        signature_value: String,
        key_info: KeyInfo,
        // Current certificate validity is mandatory. Timestamps never extend trust.
        #[serde(default, rename = "counterSign")]
        _counter_sign: Option<serde_json::Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct SignInfo {
        signature_method: String,
        digest_method: String,
        digest_value: String,
        canonicalization: String,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct KeyInfo {
        key_value: KeyValue,
        x509_data: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct KeyValue {
        rsa_key_value: RsaKeyValue,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct RsaKeyValue {
        modulus: String,
        exponent: String,
    }

    /// Retain original byte offsets and reject duplicate top-level JSON members.
    struct RawObject<'a>(Vec<(String, &'a RawValue)>);

    impl<'de> Deserialize<'de> for RawObject<'de> {
        fn deserialize<D: Deserializer<'de>>(
            deserializer: D,
        ) -> std::result::Result<Self, D::Error> {
            struct ObjectVisitor;
            impl<'de> Visitor<'de> for ObjectVisitor {
                type Value = RawObject<'de>;
                fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                    formatter.write_str("a JSON object with unique members")
                }
                fn visit_map<M: MapAccess<'de>>(
                    self,
                    mut map: M,
                ) -> std::result::Result<Self::Value, M::Error> {
                    let mut seen = HashSet::new();
                    let mut entries = Vec::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if !seen.insert(key.clone()) {
                            return Err(serde::de::Error::custom("duplicate signed JSON member"));
                        }
                        entries.push((key, map.next_value::<&RawValue>()?));
                    }
                    Ok(RawObject(entries))
                }
            }
            deserializer.deserialize_map(ObjectVisitor)
        }
    }

    impl<'a> RawObject<'a> {
        fn member(&self, name: &str) -> Result<&'a RawValue> {
            self.0
                .iter()
                .find(|(key, _)| key == name)
                .map(|(_, value)| *value)
                .ok_or_else(|| invalid(format!("missing signed {name}")))
        }
    }

    fn invalid(reason: impl fmt::Display) -> MsvcKitError {
        MsvcKitError::Config(format!(
            "Microsoft manifest authentication failed: {reason}"
        ))
    }

    fn decode(value: &str) -> Result<Vec<u8>> {
        STANDARD
            .decode(value)
            .map_err(|_| invalid("invalid signature base64"))
    }

    fn string_line(name: &str, value: &str, last: bool) -> String {
        format!(
            "\t\"{name}\" : \"{value}\"{}\r\n",
            if last { "" } else { "," }
        )
    }

    fn nested_lines(name: &str, children: Vec<String>, last: bool) -> Vec<String> {
        let mut lines = vec![format!("\t\"{name}\" : {{\r\n")];
        lines.extend(children.into_iter().map(|line| format!("\t\t{line}")));
        lines.push(format!("\t}}{}\r\n", if last { "" } else { "," }));
        lines
    }

    /// Microsoft's CNG signed-JSON schema serializes SignInfo and KeyInfo with
    /// fixed tabs/CRLF, independently of the surrounding JSON whitespace.
    fn signed_info(signature: &Signature) -> Vec<u8> {
        let info = &signature.sign_info;
        let rsa = &signature.key_info.key_value.rsa_key_value;
        let mut text = [
            string_line("signatureMethod", &info.signature_method, false),
            string_line("digestMethod", &info.digest_method, false),
            string_line("digestValue", &info.digest_value, false),
            string_line("canonicalization", &info.canonicalization, true),
        ]
        .concat();
        let rsa_lines = vec![
            string_line("modulus", &rsa.modulus, false),
            string_line("exponent", &rsa.exponent, true),
        ];
        text.push_str(
            &nested_lines(
                "keyValue",
                nested_lines("rsaKeyValue", rsa_lines, true),
                false,
            )
            .concat(),
        );
        text.push_str("\t\"x509Data\" : [\r\n");
        for (index, certificate) in signature.key_info.x509_data.iter().enumerate() {
            text.push_str(&format!(
                "\t\t\"{certificate}\"{}\r\n",
                if index + 1 == signature.key_info.x509_data.len() {
                    ""
                } else {
                    ","
                }
            ));
        }
        text.push_str("\t]\r\n");
        text.into_bytes()
    }

    fn verify_envelope<'a, F>(bytes: &'a [u8], verify_signature: &F) -> Result<RawObject<'a>>
    where
        F: Fn(&[Vec<u8>], &[u8], &[u8]) -> Result<()>,
    {
        let object: RawObject<'_> = serde_json::from_slice(bytes)?;
        let (key, raw) = object
            .0
            .last()
            .ok_or_else(|| invalid("empty signed object"))?;
        if key != "signature" || object.0.len() < 2 {
            return Err(invalid("signature must be the final JSON member"));
        }
        let signature: Signature = serde_json::from_str(raw.get())?;
        let info = &signature.sign_info;
        if info.signature_method != "sha256RSA_cng"
            || info.digest_method != "sha256"
            || !info.canonicalization.is_empty()
        {
            return Err(invalid(
                "unsupported signature algorithm or canonicalization",
            ));
        }
        // RawValue borrows this exact input. The final member's value starts after
        // its key/colon; the last comma before it terminates the signed content.
        let offset = (raw.get().as_ptr() as usize)
            .checked_sub(bytes.as_ptr() as usize)
            .filter(|offset| *offset < bytes.len())
            .ok_or_else(|| invalid("invalid signature offset"))?;
        let end = bytes[..offset]
            .iter()
            .rposition(|byte| *byte == b',')
            .map(|index| index + 1)
            .ok_or_else(|| invalid("missing signature delimiter"))?;
        let digest = decode(&info.digest_value)?;
        if digest.len() != 32
            || hex::encode(digest) != crate::downloader::hash::compute_hash(&bytes[..end])
        {
            return Err(invalid("signed content digest mismatch"));
        }
        // Strict base64 also ensures schema values cannot inject lines into the
        // deterministic signature blob. RSA verification uses the certificate key.
        let rsa = &signature.key_info.key_value.rsa_key_value;
        if decode(&rsa.modulus)?.is_empty()
            || decode(&rsa.exponent)?.is_empty()
            || signature.key_info.x509_data.is_empty()
            || signature.key_info.x509_data.len() > 8
        {
            return Err(invalid("invalid signing key or certificate count"));
        }
        let certificates = signature
            .key_info
            .x509_data
            .iter()
            .map(|certificate| decode(certificate))
            .collect::<Result<Vec<_>>>()?;
        verify_signature(
            &certificates,
            &decode(&signature.signature_value)?,
            &signed_info(&signature),
        )?;
        Ok(object)
    }

    #[derive(Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    struct ReleaseInfo {
        build_version: String,
        product_line: String,
        product_semantic_version: String,
        manifest_type: String,
        manifest_name: String,
    }

    pub(super) fn verify_catalog_with<F>(catalog: &[u8], channel: &[u8], verifier: &F) -> Result<()>
    where
        F: Fn(&[Vec<u8>], &[u8], &[u8]) -> Result<()>,
    {
        let catalog = verify_envelope(catalog, verifier)?;
        let channel = verify_envelope(channel, verifier)?;
        let package_info: ReleaseInfo = serde_json::from_str(catalog.member("info")?.get())?;
        let channel_info: ReleaseInfo = serde_json::from_str(channel.member("info")?.get())?;
        let channel_items: Vec<crate::downloader::manifest::ChannelItem> =
            serde_json::from_str(channel.member("channelItems")?.get())?;
        let item_version = channel_items
            .iter()
            .find(|item| item.id == "Microsoft.VisualStudio.Manifests.VisualStudio")
            .and_then(|item| item.version.as_deref());
        if package_info.manifest_type != "installer"
            || package_info.manifest_name != "VisualStudio"
            || channel_info.manifest_type != "channel"
            || channel_info.manifest_name
                != format!(
                    "VisualStudio.{}.Release",
                    package_info.product_line.strip_prefix("Dev").unwrap_or("")
                )
            || package_info.build_version.is_empty()
            || package_info.product_semantic_version.is_empty()
            || package_info.product_line.is_empty()
            || package_info.build_version != channel_info.build_version
            || item_version != Some(package_info.build_version.as_str())
            || package_info.product_semantic_version != channel_info.product_semantic_version
            || package_info.product_line != channel_info.product_line
        {
            return Err(invalid(
                "catalog does not match the fresh signed channel release",
            ));
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use serde_json::json;

        // These synthetic envelopes exercise structure/release binding only. They
        // cannot pass the production Windows certificate or RSA verifier.
        fn envelope(fields: &str) -> Vec<u8> {
            let prefix = format!("{{{fields},");
            let digest =
                hex::decode(crate::downloader::hash::compute_hash(prefix.as_bytes())).unwrap();
            let signature = json!({
                "signInfo": {"signatureMethod": "sha256RSA_cng", "digestMethod": "sha256",
                    "digestValue": STANDARD.encode(digest), "canonicalization": ""},
                "signatureValue": "AQ==",
                "keyInfo": {"keyValue": {"rsaKeyValue": {"modulus": "AQ==", "exponent": "AQAB"}},
                    "x509Data": ["AQ=="]}
            });
            format!("{prefix}\"signature\":{signature}}}").into_bytes()
        }

        fn structure_only(_: &[Vec<u8>], _: &[u8], _: &[u8]) -> Result<()> {
            Ok(())
        }

        fn release(catalog: bool, build: &str, line: &str, semantic: &str, item: &str) -> Vec<u8> {
            let info = json!({"buildVersion": build, "productLine": line,
            "productSemanticVersion": semantic, "manifestType": if catalog {"installer"} else {"channel"},
            "manifestName": if catalog {"VisualStudio"} else {"VisualStudio.17.Release"}});
            let items = json!([{"id": "Microsoft.VisualStudio.Manifests.VisualStudio", "type": "Manifest", "version": item}]);
            envelope(&format!("\"info\":{info},\"channelItems\":{items}"))
        }

        #[test]
        fn release_binding_rejects_other_build_product_and_channel_item() {
            let catalog = release(true, "17.14.1", "Dev17", "17.14.1+1", "17.14.1");
            let channel = release(false, "17.14.1", "Dev17", "17.14.1+1", "17.14.1");
            verify_catalog_with(&catalog, &channel, &structure_only).unwrap();
            for channel in [
                release(false, "17.14.2", "Dev17", "17.14.1+1", "17.14.1"),
                release(false, "17.14.1", "Dev18", "17.14.1+1", "17.14.1"),
                release(false, "17.14.1", "Dev17", "17.14.2+1", "17.14.1"),
                release(false, "17.14.1", "Dev17", "17.14.1+1", "17.14.2"),
                release(true, "17.14.1", "Dev17", "17.14.1+1", "17.14.1"),
            ] {
                assert!(verify_catalog_with(&catalog, &channel, &structure_only).is_err());
            }
            let untrusted = |_: &[Vec<u8>], _: &[u8], _: &[u8]| Err(invalid("untrusted signer"));
            assert!(verify_catalog_with(&catalog, &channel, &untrusted).is_err());
        }

        #[test]
        fn rejects_ambiguous_json_and_unsigned_trailing_members() {
            for fields in [r#""data":1,"data":2"#, r#""data":1,"\u0064ata":2"#] {
                assert!(verify_envelope(&envelope(fields), &structure_only).is_err());
            }
            let valid = String::from_utf8(envelope(r#""data":1"#)).unwrap();
            let trailing = format!("{},\"unsigned\":true}}", &valid[..valid.len() - 1]);
            assert!(verify_envelope(trailing.as_bytes(), &structure_only).is_err());
            let duplicate = valid.replace(
                "\"signatureValue\":",
                "\"signatureValue\":\"AQ==\",\"signatureValue\":",
            );
            assert!(verify_envelope(duplicate.as_bytes(), &structure_only).is_err());
        }

        #[test]
        fn rejects_content_algorithm_and_base64_changes_before_crypto() {
            let valid = String::from_utf8(envelope(r#""data":1"#)).unwrap();
            for invalid in [
                valid.replace("\"data\":1", "\"data\":2"),
                valid.replace("sha256RSA_cng", "sha1RSA_cng"),
                valid.replace("AQAB", "not base64"),
            ] {
                assert!(verify_envelope(invalid.as_bytes(), &structure_only).is_err());
            }
        }

        #[cfg(windows)]
        #[test]
        fn microsoft_fixture_verifies_exact_signed_bytes_and_rejects_rsa_tampering() {
            let bytes = include_bytes!("../../tests/fixtures/visualstudio-17-release.channel.json");
            let rsa = |certificates: &[Vec<u8>], signature: &[u8], info: &[u8]| {
                crate::downloader::manifest_trust_windows::verify_rsa(
                    &certificates[0],
                    signature,
                    info,
                )
            };
            verify_envelope(bytes, &rsa).unwrap();
            let changed_info = |certificates: &[Vec<u8>], signature: &[u8], info: &[u8]| {
                let mut changed = info.to_vec();
                changed[0] ^= 1;
                rsa(certificates, signature, &changed)
            };
            assert!(verify_envelope(bytes, &changed_info).is_err());
            let changed_signature = |certificates: &[Vec<u8>], signature: &[u8], info: &[u8]| {
                let mut changed = signature.to_vec();
                changed[0] ^= 1;
                rsa(certificates, &changed, info)
            };
            assert!(verify_envelope(bytes, &changed_signature).is_err());
            let wrong_signer = |certificates: &[Vec<u8>], signature: &[u8], info: &[u8]| {
                crate::downloader::manifest_trust_windows::verify_rsa(
                    &certificates[1],
                    signature,
                    info,
                )
            };
            assert!(verify_envelope(bytes, &wrong_signer).is_err());
        }
    }
}
