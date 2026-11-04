use crate::errors::Error;

pub const MAX_STATE_BYTES: usize = 1024 * 1024;
const HEADER_LEN: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateVersion(pub u16);

impl StateVersion {
    pub const V1: Self = Self(1);
}

/// Versioned provider state. Persistence owns when a staged value becomes live.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateEnvelope {
    pub version: StateVersion,
    pub generation: u64,
    pub provider: Vec<u8>,
}

impl StateEnvelope {
    pub fn new(generation: u64, provider: Vec<u8>) -> Result<Self, Error> {
        if provider.len() > MAX_STATE_BYTES {
            return Err(Error::StateTooLarge);
        }
        Ok(Self {
            version: StateVersion::V1,
            generation,
            provider,
        })
    }

    pub fn initial(provider: Vec<u8>) -> Result<Self, Error> {
        Self::new(0, provider)
    }

    pub fn encode(&self) -> Result<Vec<u8>, Error> {
        if self.version != StateVersion::V1 || self.provider.len() > MAX_STATE_BYTES {
            return Err(Error::StateEncoding);
        }
        let provider_len = u32::try_from(self.provider.len()).map_err(|_| Error::StateEncoding)?;
        let mut out = Vec::with_capacity(HEADER_LEN + self.provider.len());
        out.extend_from_slice(b"KAMI-STATE\0");
        out.extend_from_slice(&self.version.0.to_be_bytes());
        out.extend_from_slice(&self.generation.to_be_bytes());
        out.extend_from_slice(&provider_len.to_be_bytes());
        out.extend_from_slice(&self.provider);
        Ok(out)
    }

    pub fn decode(bytes: &[u8], expected_generation: u64) -> Result<Self, Error> {
        if bytes.len() < HEADER_LEN || &bytes[..11] != b"KAMI-STATE\0" {
            return Err(Error::StateEncoding);
        }
        let version =
            u16::from_be_bytes(bytes[11..13].try_into().map_err(|_| Error::StateEncoding)?);
        let generation =
            u64::from_be_bytes(bytes[13..21].try_into().map_err(|_| Error::StateEncoding)?);
        let len = u32::from_be_bytes(bytes[21..25].try_into().map_err(|_| Error::StateEncoding)?)
            as usize;
        if version != StateVersion::V1.0 {
            return Err(Error::StateVersion {
                found: version,
                expected: StateVersion::V1.0,
            });
        }
        if generation != expected_generation {
            return Err(Error::StateRollback {
                found: generation,
                expected: expected_generation,
            });
        }
        if len > MAX_STATE_BYTES || bytes.len() != HEADER_LEN + len {
            return Err(Error::StateEncoding);
        }
        Self::new(generation, bytes[HEADER_LEN..].to_vec())
    }
}
