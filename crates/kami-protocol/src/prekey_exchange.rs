//! Strict prekey-only protobuf v1. Structural decoding grants no authentication.
//! The four-byte transport length prefix counts toward the 16-KiB frame bound.
use std::fmt;

pub const MAX_FRAME_BYTES: usize = 16 * 1024;
pub const MAX_BODY_BYTES: usize = MAX_FRAME_BYTES - 4;
pub const MAX_PROOF_BYTES: usize = 1024;
/// A single signed selection avoids partial-batch publication semantics.
pub const MAX_SELECTIONS: usize = 1;

#[derive(Clone, Eq, PartialEq)]
pub struct Query {
    pub request: [u8; 32],
    pub requester: [u8; 32],
    pub publisher: [u8; 32],
    pub minimum_generation: u64,
}
#[derive(Clone, Eq, PartialEq)]
pub struct Selection {
    pub request: [u8; 32],
    pub publisher: [u8; 32],
    pub recipient: [u8; 32],
    pub account_generation: u64,
    pub bundle_generation: u64,
    /// Exact bounded OlmBindingProof encoding; cryptographic validation is E2EE's responsibility.
    pub proof: Vec<u8>,
}
#[derive(Clone, Eq, PartialEq)]
pub struct Acknowledgement {
    pub request: [u8; 32],
    pub sender: [u8; 32],
    pub recipient: [u8; 32],
    pub bundle_generation: u64,
    pub response_digest: [u8; 32],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Failure {
    Busy = 1,
    Unauthorized,
    Expired,
    Revoked,
    Depleted,
    CustodyUnavailable,
    Conflict,
    Indeterminate,
    Cancelled,
    Malformed,
    Unsupported,
}
#[derive(Clone, Eq, PartialEq)]
pub enum Exchange {
    Query(Query),
    Selection(Selection),
    Ack(Acknowledgement),
    AckResult(Acknowledgement),
    Error { request: [u8; 32], failure: Failure },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidExchange;
impl fmt::Display for InvalidExchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid prekey exchange")
    }
}
impl std::error::Error for InvalidExchange {}
impl fmt::Debug for Exchange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Exchange([REDACTED])")
    }
}
impl Exchange {
    pub fn request_id(&self) -> &[u8; 32] {
        match self {
            Self::Query(q) => &q.request,
            Self::Selection(s) => &s.request,
            Self::Ack(a) | Self::AckResult(a) => &a.request,
            Self::Error { request, .. } => request,
        }
    }
}
fn varint(out: &mut Vec<u8>, mut n: u64) {
    while n >= 128 {
        out.push(n as u8 | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
fn number(out: &mut Vec<u8>, tag: u8, n: u64) {
    out.push(tag << 3);
    varint(out, n);
}
fn bytes(out: &mut Vec<u8>, tag: u8, value: &[u8]) {
    out.push(tag << 3 | 2);
    varint(out, value.len() as u64);
    out.extend(value);
}
fn generation(n: u64) -> Result<u64, InvalidExchange> {
    if n == 0 || n > i64::MAX as u64 {
        Err(InvalidExchange)
    } else {
        Ok(n)
    }
}
fn nonzero(v: &[u8; 32]) -> Result<(), InvalidExchange> {
    if *v == [0; 32] {
        Err(InvalidExchange)
    } else {
        Ok(())
    }
}
pub fn encode(value: &Exchange) -> Result<Vec<u8>, InvalidExchange> {
    nonzero(value.request_id())?;
    let mut out = Vec::with_capacity(256);
    number(&mut out, 1, 1);
    let kind = match value {
        Exchange::Query(_) => 1,
        Exchange::Selection(_) => 2,
        Exchange::Ack(_) => 3,
        Exchange::AckResult(_) => 4,
        Exchange::Error { .. } => 5,
    };
    number(&mut out, 2, kind);
    bytes(&mut out, 3, value.request_id());
    match value {
        Exchange::Query(q) => {
            nonzero(&q.requester)?;
            nonzero(&q.publisher)?;
            if q.requester == q.publisher {
                return Err(InvalidExchange);
            }
            bytes(&mut out, 4, &q.requester);
            bytes(&mut out, 5, &q.publisher);
            number(&mut out, 6, generation(q.minimum_generation)?);
        }
        Exchange::Selection(s) => {
            nonzero(&s.publisher)?;
            nonzero(&s.recipient)?;
            if s.publisher == s.recipient || s.proof.is_empty() || s.proof.len() > MAX_PROOF_BYTES {
                return Err(InvalidExchange);
            }
            bytes(&mut out, 4, &s.publisher);
            bytes(&mut out, 5, &s.recipient);
            number(&mut out, 6, generation(s.bundle_generation)?);
            number(&mut out, 7, generation(s.account_generation)?);
            bytes(&mut out, 8, &s.proof);
        }
        Exchange::Ack(a) | Exchange::AckResult(a) => {
            nonzero(&a.sender)?;
            nonzero(&a.recipient)?;
            nonzero(&a.response_digest)?;
            if a.sender == a.recipient {
                return Err(InvalidExchange);
            }
            bytes(&mut out, 4, &a.sender);
            bytes(&mut out, 5, &a.recipient);
            number(&mut out, 6, generation(a.bundle_generation)?);
            bytes(&mut out, 9, &a.response_digest);
        }
        Exchange::Error { failure, .. } => number(&mut out, 10, *failure as u64),
    }
    if out.len() > MAX_BODY_BYTES {
        return Err(InvalidExchange);
    }
    Ok(out)
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn varint(&mut self) -> Result<u64, InvalidExchange> {
        let mut n = 0u64;
        for i in 0..10 {
            let (&b, tail) = self.0.split_first().ok_or(InvalidExchange)?;
            self.0 = tail;
            if i == 9 && b > 1 {
                return Err(InvalidExchange);
            }
            n |= u64::from(b & 127) << (i * 7);
            if b < 128 {
                if i > 0 && b == 0 {
                    return Err(InvalidExchange);
                }
                return Ok(n);
            }
        }
        Err(InvalidExchange)
    }
    fn tag(&mut self, tag: u8, wire: u8) -> Result<(), InvalidExchange> {
        if self.varint()? != u64::from(tag << 3 | wire) {
            return Err(InvalidExchange);
        }
        Ok(())
    }
    fn number(&mut self, tag: u8) -> Result<u64, InvalidExchange> {
        self.tag(tag, 0)?;
        self.varint()
    }
    fn bytes(&mut self, tag: u8, max: usize) -> Result<&'a [u8], InvalidExchange> {
        self.tag(tag, 2)?;
        let n = usize::try_from(self.varint()?).map_err(|_| InvalidExchange)?;
        if n == 0 || n > max {
            return Err(InvalidExchange);
        }
        let value = self.0.get(..n).ok_or(InvalidExchange)?;
        self.0 = &self.0[n..];
        Ok(value)
    }
    fn fixed(&mut self, tag: u8) -> Result<[u8; 32], InvalidExchange> {
        self.bytes(tag, 32)?.try_into().map_err(|_| InvalidExchange)
    }
}
pub fn decode(input: &[u8]) -> Result<Exchange, InvalidExchange> {
    if input.len() > MAX_BODY_BYTES {
        return Err(InvalidExchange);
    }
    let mut r = Reader(input);
    if r.number(1)? != 1 {
        return Err(InvalidExchange);
    }
    let kind = r.number(2)?;
    let request = r.fixed(3)?;
    let value = match kind {
        1 => Exchange::Query(Query {
            request,
            requester: r.fixed(4)?,
            publisher: r.fixed(5)?,
            minimum_generation: r.number(6)?,
        }),
        2 => Exchange::Selection(Selection {
            request,
            publisher: r.fixed(4)?,
            recipient: r.fixed(5)?,
            bundle_generation: r.number(6)?,
            account_generation: r.number(7)?,
            proof: r.bytes(8, MAX_PROOF_BYTES)?.to_vec(),
        }),
        3 | 4 => {
            let a = Acknowledgement {
                request,
                sender: r.fixed(4)?,
                recipient: r.fixed(5)?,
                bundle_generation: r.number(6)?,
                response_digest: r.fixed(9)?,
            };
            if kind == 3 {
                Exchange::Ack(a)
            } else {
                Exchange::AckResult(a)
            }
        }
        5 => {
            let failure = match r.number(10)? {
                1 => Failure::Busy,
                2 => Failure::Unauthorized,
                3 => Failure::Expired,
                4 => Failure::Revoked,
                5 => Failure::Depleted,
                6 => Failure::CustodyUnavailable,
                7 => Failure::Conflict,
                8 => Failure::Indeterminate,
                9 => Failure::Cancelled,
                10 => Failure::Malformed,
                11 => Failure::Unsupported,
                _ => return Err(InvalidExchange),
            };
            Exchange::Error { request, failure }
        }
        _ => return Err(InvalidExchange),
    };
    if !r.0.is_empty() || encode(&value)? != input {
        return Err(InvalidExchange);
    }
    Ok(value)
}
