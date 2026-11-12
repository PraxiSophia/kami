//! Direct authenticated connection substrate. No application protocols or payloads.
mod connection;
mod driver;
mod prekey_exchange;
pub use connection::{AuthenticatedConnection, Revocation};
pub use driver::{Driver, Event, Handle};
pub use prekey_exchange::{IncomingPrekeyExchange, LivePrekeyExchange};
mod policy;
pub use libp2p::Multiaddr;
#[cfg(test)]
use policy::direct_address;
pub use policy::Config;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidConfig,
    InvalidAddress,
    Busy,
    Closed,
    Untrusted,
    Unavailable,
    Timeout,
    Cancelled,
    Expired,
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direct_addresses_and_resource_policy_are_bounded() {
        let config = Config::default();
        assert!(config.validate().is_ok());
        for address in ["/ip4/127.0.0.1/tcp/1234", "/ip6/::1/udp/1234/quic-v1"] {
            assert!(direct_address(&address.parse().unwrap(), false).is_ok());
        }
        for address in [
            "/dns/example.com/tcp/1234",
            "/ip4/127.0.0.1/udp/1234",
            "/ip4/127.0.0.1/tcp/0",
            "/ip4/127.0.0.1/tcp/1/ws",
        ] {
            assert!(direct_address(&address.parse().unwrap(), false).is_err());
        }
        for config in [
            Config {
                max_connections: 0,
                ..Config::default()
            },
            Config {
                max_attempts: 9,
                ..Config::default()
            },
            Config {
                dial_timeout: std::time::Duration::ZERO,
                ..Config::default()
            },
        ] {
            assert!(config.validate().is_err());
        }
    }
}
