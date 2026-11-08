use crate::{
    store::read_key, verify_enrollment_for_transport, KeyPurpose, KeySlot, NativeCustody,
    VerifiedEnrollment,
};
use kami_key_provider::KeyError;
use libp2p_core::{muxing::StreamMuxerBox, transport::Boxed, upgrade, Transport};
use libp2p_identity::{Keypair, PeerId};
use std::time::Duration;

/// Move-only private configuration pair. No key, config, signer or clone escape.
/// ```compile_fail
/// let _ = kami_identity::SealedTransportCredentials { quic: todo!(), noise: todo!(), local: todo!(), invitation: todo!() };
/// ```
/// ```compile_fail
/// fn duplicate(c: kami_identity::SealedTransportCredentials) { let _ = c.clone(); }
/// ```
/// ```compile_fail
/// fn expose(c: kami_identity::SealedTransportCredentials) { println!("{c:?}"); }
/// ```
/// ```compile_fail
/// fn expose(c: kami_identity::SealedTransportCredentials) { let _ = c.into_keypair(); }
/// ```
/// ```compile_fail
/// fn expose(c: kami_identity::SealedTransportCredentials) { let _ = c.noise; }
/// ```
pub struct SealedTransportCredentials {
    quic: libp2p_quic::Config,
    noise: libp2p_noise::Config,
    local: PeerId,
    invitation: crate::Invitation,
}
impl VerifiedEnrollment {
    /// Caller must obtain fresh_row from the still lease-holding storage worker.
    /// Re-read anchor and keys internally; never accept an already-verified flag.
    pub fn materialize(
        self,
        custody: &NativeCustody,
        fresh_row: &[u8],
    ) -> Result<SealedTransportCredentials, KeyError> {
        if self.row != fresh_row {
            return Err(KeyError::Corrupt);
        }
        verify_enrollment_for_transport(custody, &self.profile, fresh_row)?;
        let mut seed = read_key(
            &custody.custody.0,
            &KeySlot::new(self.profile.clone(), KeyPurpose::Identity),
        )?;
        let pair = Keypair::ed25519_from_bytes(seed.as_mut()).map_err(|_| KeyError::Corrupt)?;
        let public = pair
            .public()
            .try_into_ed25519()
            .map_err(|_| KeyError::Corrupt)?;
        if public.to_bytes().as_slice() != &self.row[34..] {
            return Err(KeyError::Corrupt);
        }
        let local = pair.public().to_peer_id();
        let record = crate::EnrollmentRecord::from_parts(
            KeySlot::new(self.profile.clone(), KeyPurpose::Identity),
            KeySlot::new(self.profile, KeyPurpose::Database),
            public.to_bytes(),
        )?;
        let binding = custody
            .custody
            .create_peer_binding(&record, &pair.public())
            .map_err(|_| KeyError::Corrupt)?;
        let invitation =
            crate::Invitation::new(public.to_bytes(), binding).map_err(|_| KeyError::Corrupt)?;
        let mut quic = libp2p_quic::Config::new(&pair);
        quic.handshake_timeout = Duration::from_secs(5);
        quic.max_concurrent_stream_limit = 1;
        let noise = libp2p_noise::Config::new(&pair).map_err(|_| KeyError::Corrupt)?;
        // QUIC Config clones Keypair until consumed into GenTransport; conversion
        // derives TLS/reset secrets. Noise retains signed static-DH secret. Drop
        // the local pair/seed now; this is NOT process-compromise non-exportability.
        Ok(SealedTransportCredentials {
            quic,
            noise,
            local,
            invitation,
        })
    }
}
impl SealedTransportCredentials {
    pub fn invitation(&self) -> &crate::Invitation {
        &self.invitation
    }
    pub fn local_peer_id(&self) -> PeerId {
        self.local
    }
    /// Pure construction: no listen, dial, socket or task. Socket/lifecycle
    /// ownership stays with the consuming transport and runtime supervisor.
    pub fn into_transport(self) -> Boxed<(PeerId, StreamMuxerBox)> {
        let tcp = libp2p_tcp::tokio::Transport::new(libp2p_tcp::Config::default().nodelay(true))
            .upgrade(upgrade::Version::V1Lazy)
            .authenticate(self.noise)
            .multiplex(libp2p_yamux::Config::default())
            .timeout(Duration::from_secs(5))
            .map(|(peer, mux), _| (peer, StreamMuxerBox::new(mux)))
            .boxed();
        libp2p_quic::tokio::Transport::new(self.quic)
            .map(|(peer, mux), _| (peer, StreamMuxerBox::new(mux)))
            .or_transport(tcp)
            .map(|either, _| either.into_inner())
            .boxed()
    }
}
