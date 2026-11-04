use crate::{errors::Error, types::ApplicationContext};

pub const MAX_PROFILE_BYTES: usize = 64 * 1024;
pub const MAX_PLAINTEXT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileVersion(pub u16);

impl ProfileVersion {
    pub const V1: Self = Self(1);
}

/// Session metadata only; skipped-key retention and prekey provisioning are not
/// configurable Kami guarantees in this foundation.
/// ```compile_fail
/// fn unsupported(profile: kami_e2ee::Profile) {
///     let _ = profile.max_skipped_keys;
/// }
/// ```
/// ```compile_fail
/// fn unsupported(profile: kami_e2ee::Profile) {
///     let _ = profile.max_prekey_batch;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub version: ProfileVersion,
    pub context: ApplicationContext,
}

impl Profile {
    pub fn v1(context: ApplicationContext) -> Self {
        Self {
            version: ProfileVersion::V1,
            context,
        }
    }

    pub fn validate(&self) -> Result<(), Error> {
        if self.version != ProfileVersion::V1 {
            return Err(Error::InvalidContext);
        }
        self.context.validate()
    }

    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let ctx = self.context.encode()?;
        let len = u16::try_from(ctx.len()).map_err(|_| Error::InvalidContext)?;
        let mut out = Vec::with_capacity(16 + ctx.len());
        out.extend_from_slice(b"KAMI-E2EE");
        out.extend_from_slice(&self.version.0.to_be_bytes());

        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&ctx);
        if out.len() > MAX_PROFILE_BYTES {
            return Err(Error::StateTooLarge);
        }
        Ok(out)
    }
}
