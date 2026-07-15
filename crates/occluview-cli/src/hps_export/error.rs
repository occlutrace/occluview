use occluview_formats::hps::HpsReadFailure;
use serde::Serialize;
use std::io::Write;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub(crate) enum CliError {
    InvalidArguments,
    InputReadFailed,
    InputTooLarge,
    KeyMissing,
    InvalidKey,
    KeyProviderFailed,
    MedicalDicom,
    BadSignature,
    MalformedInput,
    UnsupportedEncoding,
    IntegrityFailure,
    PackageLocked,
    TextureMalformed,
    ResourceLimit,
    SurfaceConversionFailed,
    ArtifactEncodingFailed,
    OutputDirectoryFailed,
    OutputExists,
    OutputWriteFailed,
    ManifestEncodingFailed,
    StdoutWriteFailed,
}

#[derive(Serialize)]
struct ErrorEnvelope {
    schema_version: u32,
    error: ErrorBody,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: &'static str,
    exit_code: u8,
}

impl CliError {
    pub(crate) const fn invalid_arguments() -> Self {
        Self::InvalidArguments
    }

    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::InvalidArguments => "invalid_arguments",
            Self::InputReadFailed => "input_read_failed",
            Self::InputTooLarge => "input_too_large",
            Self::KeyMissing => "key_missing",
            Self::InvalidKey => "invalid_key",
            Self::KeyProviderFailed => "key_provider_failed",
            Self::MedicalDicom => "medical_dicom_unsupported",
            Self::BadSignature => "bad_signature",
            Self::MalformedInput => "malformed_input",
            Self::UnsupportedEncoding => "unsupported_encoding",
            Self::IntegrityFailure => "integrity_failure",
            Self::PackageLocked => "package_locked",
            Self::TextureMalformed => "texture_malformed",
            Self::ResourceLimit => "resource_limit",
            Self::SurfaceConversionFailed => "surface_conversion_failed",
            Self::ArtifactEncodingFailed => "artifact_encoding_failed",
            Self::OutputDirectoryFailed => "output_directory_failed",
            Self::OutputExists => "output_exists",
            Self::OutputWriteFailed => "output_write_failed",
            Self::ManifestEncodingFailed => "manifest_encoding_failed",
            Self::StdoutWriteFailed => "stdout_write_failed",
        }
    }

    pub(crate) const fn exit_code(self) -> u8 {
        match self {
            Self::InvalidArguments => 2,
            Self::InputReadFailed | Self::InputTooLarge => 3,
            Self::KeyMissing | Self::InvalidKey | Self::KeyProviderFailed => 4,
            Self::MedicalDicom
            | Self::BadSignature
            | Self::MalformedInput
            | Self::UnsupportedEncoding
            | Self::IntegrityFailure
            | Self::PackageLocked
            | Self::TextureMalformed
            | Self::ResourceLimit => 5,
            Self::OutputDirectoryFailed
            | Self::OutputExists
            | Self::OutputWriteFailed
            | Self::ManifestEncodingFailed
            | Self::StdoutWriteFailed => 6,
            Self::SurfaceConversionFailed | Self::ArtifactEncodingFailed => 7,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::InvalidArguments => "invalid command-line arguments",
            Self::InputReadFailed => "input could not be read",
            Self::InputTooLarge => "input exceeds the supported size limit",
            Self::KeyMissing => "required HPS decryption key is not configured",
            Self::InvalidKey => "configured HPS decryption key is invalid",
            Self::KeyProviderFailed => "HPS decryption key is unavailable",
            Self::MedicalDicom => "medical DICOM is not a dental HPS surface",
            Self::BadSignature => "input is not a recognized dental HPS surface",
            Self::MalformedInput => "dental HPS input is malformed",
            Self::UnsupportedEncoding => "dental HPS encoding is unsupported",
            Self::IntegrityFailure => "dental HPS integrity validation failed",
            Self::PackageLocked => "dental HPS package is locked",
            Self::TextureMalformed => "dental HPS texture data is malformed",
            Self::ResourceLimit => "dental HPS input exceeds a resource limit",
            Self::SurfaceConversionFailed => "decoded surface could not be converted",
            Self::ArtifactEncodingFailed => "surface artifact could not be encoded",
            Self::OutputDirectoryFailed => "output directory is unavailable",
            Self::OutputExists => "a reserved output artifact already exists",
            Self::OutputWriteFailed => "output artifact could not be written",
            Self::ManifestEncodingFailed => "output manifest could not be encoded",
            Self::StdoutWriteFailed => "output manifest could not be emitted",
        }
    }

    pub(crate) fn write_json(self, writer: &mut impl Write) {
        let envelope = ErrorEnvelope {
            schema_version: 1,
            error: ErrorBody {
                code: self.code(),
                message: self.message(),
                exit_code: self.exit_code(),
            },
        };
        if serde_json::to_writer(&mut *writer, &envelope).is_ok() {
            let _ = writer.write_all(b"\n");
        }
    }

    pub(crate) const fn from_parser(error: HpsReadFailure) -> Self {
        match error {
            HpsReadFailure::MedicalDicom => Self::MedicalDicom,
            HpsReadFailure::BadSignature => Self::BadSignature,
            HpsReadFailure::MalformedInput => Self::MalformedInput,
            HpsReadFailure::UnsupportedEncoding => Self::UnsupportedEncoding,
            HpsReadFailure::KeyMissing => Self::KeyMissing,
            HpsReadFailure::InvalidKey => Self::InvalidKey,
            HpsReadFailure::IntegrityFailure => Self::IntegrityFailure,
            HpsReadFailure::PackageLocked => Self::PackageLocked,
            HpsReadFailure::TextureMalformed => Self::TextureMalformed,
            HpsReadFailure::ResourceLimit => Self::ResourceLimit,
            HpsReadFailure::KeyProviderFailed => Self::KeyProviderFailed,
        }
    }

    pub(crate) const fn from_key_provider(error: HpsReadFailure) -> Self {
        match error {
            HpsReadFailure::InvalidKey => Self::InvalidKey,
            _ => Self::KeyProviderFailed,
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::CliError;
    use occluview_formats::hps::HpsReadFailure;

    #[test]
    fn parser_error_details_are_replaced_with_safe_stable_metadata() {
        let error = CliError::from_parser(HpsReadFailure::MalformedInput);
        let mut json = Vec::new();
        error.write_json(&mut json);
        let json = String::from_utf8(json).expect("JSON is UTF-8");

        assert_eq!(error.code(), "malformed_input");
        assert_eq!(error.exit_code(), 5);
        assert!(!json.contains("patient name"));
        assert!(!json.contains("raw path"));
        assert!(!json.contains("secret key"));
    }

    #[test]
    fn key_provider_failures_have_stable_key_exit_codes() {
        assert_eq!(
            CliError::from_key_provider(HpsReadFailure::InvalidKey).code(),
            "invalid_key"
        );
        assert_eq!(
            CliError::from_key_provider(HpsReadFailure::InvalidKey).exit_code(),
            4
        );

        assert_eq!(
            CliError::from_key_provider(HpsReadFailure::KeyProviderFailed).code(),
            "key_provider_failed"
        );
    }
}
