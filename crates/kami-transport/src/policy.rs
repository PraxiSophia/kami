use crate::Error;
use libp2p::{multiaddr::Protocol, Multiaddr};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Config {
    pub max_connections: u32,
    pub max_dials: usize,
    pub max_attempts: usize,
    pub dial_timeout: Duration,
    pub capability_ttl: Duration,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_connections: 16,
            max_dials: 4,
            max_attempts: 8,
            dial_timeout: Duration::from_secs(3),
            capability_ttl: Duration::from_secs(30),
        }
    }
}
impl Config {
    pub fn validate(&self) -> Result<(), Error> {
        if !(1..=32).contains(&self.max_connections)
            || !(1..=8).contains(&self.max_dials)
            || !(1..=8).contains(&self.max_attempts)
            || self.dial_timeout.is_zero()
            || self.dial_timeout > Duration::from_secs(10)
            || self.capability_ttl.is_zero()
            || self.capability_ttl > Duration::from_secs(60)
        {
            return Err(Error::InvalidConfig);
        }
        Ok(())
    }
}
/// Exact IP+TCP or IP+UDP+QUIC-v1 only. No DNS, relay, web, or asserted PeerId.
pub(crate) fn direct_address(addr: &Multiaddr, listen: bool) -> Result<bool, Error> {
    if addr.len() > 64 {
        return Err(Error::InvalidAddress);
    }
    let mut p = addr.iter();
    match p.next() {
        Some(Protocol::Ip4(ip)) if !ip.is_multicast() => {}
        Some(Protocol::Ip6(ip)) if !ip.is_multicast() => {}
        _ => return Err(Error::InvalidAddress),
    }
    let quic = match p.next() {
        Some(Protocol::Tcp(port)) if listen || port != 0 => false,
        Some(Protocol::Udp(port))
            if (listen || port != 0) && p.next() == Some(Protocol::QuicV1) =>
        {
            true
        }
        _ => return Err(Error::InvalidAddress),
    };
    if p.next().is_some() {
        return Err(Error::InvalidAddress);
    }
    Ok(quic)
}
