use crate::{Field as F, ProtocolError as E};

const FIELDS: [F; 12] = [
    F::Version,
    F::Kind,
    F::MessageId,
    F::ConversationId,
    F::Sender,
    F::Recipient,
    F::Sequence,
    F::CreatedAt,
    F::Payload,
    F::Context,
    F::ReceiptTarget,
    F::MandatoryFeatures,
];
pub(crate) const MAX_TIMESTAMP: u64 = 253_402_300_799_999; // Unix milliseconds through year 9999
pub(crate) const MAX_PAYLOAD: usize = 65_536;
pub(crate) const MAX_CONTEXT: usize = 1024;

fn varint(bytes: &[u8], pos: &mut usize) -> Result<u64, E> {
    let mut value = 0u64;
    for index in 0..10 {
        let byte = *bytes.get(*pos).ok_or(E::MalformedWire)?;
        *pos += 1;
        if index == 9 && byte > 1 {
            return Err(E::MalformedWire);
        }
        value |= u64::from(byte & 127) << (index * 7);
        if byte < 128 {
            if index > 0 && byte == 0 {
                return Err(E::NonCanonicalWire);
            }
            return Ok(value);
        }
    }
    Err(E::MalformedWire)
}

/// Flat, allocation-free whitelist scan before prost can allocate.
pub(crate) fn scan(bytes: &[u8]) -> Result<(), E> {
    let mut pos = 0;
    let mut last = 0;
    let mut present = [false; 12];
    let mut scalars = [0u64; 12];
    while pos < bytes.len() {
        let key = varint(bytes, &mut pos)?;
        let tag = key >> 3;
        if tag == 0 {
            return Err(E::MalformedWire);
        }
        if tag > 12 {
            return Err(E::UnknownField);
        }
        if tag <= last {
            return Err(E::NonCanonicalWire);
        }
        last = tag;
        let i = tag as usize - 1;
        present[i] = true;
        let is_bytes = matches!(tag, 3..=6 | 9..=11);
        if key & 7 != if is_bytes { 2 } else { 0 } {
            return Err(E::MalformedWire);
        }
        let value = varint(bytes, &mut pos)?;
        if is_bytes {
            let len = usize::try_from(value).map_err(|_| E::InvalidField(FIELDS[i]))?;
            let valid = match tag {
                3 | 4 | 11 => len == 16,
                5 | 6 => len == 32,
                9 => (1..=MAX_PAYLOAD).contains(&len),
                10 => (16..=MAX_CONTEXT).contains(&len),
                _ => false,
            };
            if !valid {
                return Err(E::InvalidField(FIELDS[i]));
            }
            pos = pos
                .checked_add(len)
                .filter(|end| *end <= bytes.len())
                .ok_or(E::MalformedWire)?;
        } else {
            scalars[i] = value;
        }
    }
    if !present[0] {
        return Err(E::MissingField(F::Version));
    }
    if scalars[0] != 1 {
        return Err(E::UnsupportedVersion);
    }
    if !present[1] {
        return Err(E::MissingField(F::Kind));
    }
    if !matches!(scalars[1], 1 | 2) {
        return Err(E::InvalidKind);
    }
    for i in [2, 3, 4, 5, 7, 8, 9] {
        if !present[i] {
            return Err(E::MissingField(FIELDS[i]));
        }
    }
    if scalars[7] > MAX_TIMESTAMP {
        return Err(E::InvalidField(F::CreatedAt));
    }
    if scalars[1] == 1 {
        if !present[6] {
            return Err(E::MissingField(F::Sequence));
        }
        if scalars[6] == 0 {
            return Err(E::InvalidField(F::Sequence));
        }
        if present[10] {
            return Err(E::InvalidField(F::ReceiptTarget));
        }
    } else {
        if present[6] {
            return Err(E::InvalidField(F::Sequence));
        }
        if !present[10] {
            return Err(E::MissingField(F::ReceiptTarget));
        }
    }
    if present[11] {
        return Err(if scalars[11] == 0 {
            E::NonCanonicalWire
        } else {
            E::UnsupportedFeature
        });
    }
    Ok(())
}
