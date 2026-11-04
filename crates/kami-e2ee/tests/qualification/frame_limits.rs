use super::*;
use vodozemac::olm::{Session, SessionConfig};
use vodozemac::Curve25519PublicKey;
fn varint(mut n: u64, out: &mut Vec<u8>) {
    while n >= 128 {
        out.push((n as u8) | 128);
        n >>= 7;
    }
    out.push(n as u8);
}
fn bytes(tag: u8, data: &[u8], out: &mut Vec<u8>) {
    out.push(tag * 8 + 2);
    varint(data.len() as u64, out);
    out.extend(data);
}
// Independent unbounded test encoder permits constructing the forbidden +1
// frame. Production ALWAYS uses kami-protocol's bounded encoder/scanner.
fn unbounded(i: &crate::envelope::InnerEnvelopeV1, ciphertext: &[u8]) -> Vec<u8> {
    let mut o = vec![8, 1, 16, 1];
    bytes(3, &i.context.message.as_u128().to_be_bytes(), &mut o);
    bytes(4, &i.context.conversation.as_u128().to_be_bytes(), &mut o);
    bytes(5, i.context.sender.as_bytes(), &mut o);
    bytes(6, i.context.recipient.as_bytes(), &mut o);
    o.push(56);
    varint(i.sequence.unwrap().value(), &mut o);
    o.push(64);
    varint(i.created_at_ms, &mut o);
    bytes(9, ciphertext, &mut o);
    bytes(10, &i.commitment().unwrap(), &mut o);
    o
}
fn raw(p: &Pair) -> Session {
    p.alice
        .create_outbound_session(
            SessionConfig::version_1(),
            Curve25519PublicKey::from_bytes(p.a.remote.selected.olm_curve25519),
            Curve25519PublicKey::from_bytes(p.a.remote.selected.prekey_curve25519),
        )
        .unwrap()
}
fn exact_frame(
    p: &Pair,
    s: &Session,
    target: usize,
) -> (crate::envelope::InnerEnvelopeV1, Vec<u8>) {
    let bound = outbound(p);
    let mut i = inner_for(p, &bound);
    i.session_id = s.session_id();
    // Inner timestamp/sequence are fixed width, whereas their outer protobuf
    // representations let us exercise an EXACT byte boundary despite AES blocks.
    for n in (64_000..65_200).step_by(8) {
        i.payload = vec![b'x'; n];
        let mut clone = Session::from_pickle(s.pickle());
        let ct = encode_ciphertext(&clone.encrypt(i.encode().unwrap()).unwrap());
        let base = unbounded(&i, &ct).len();
        if !(target.saturating_sub(24)..=target + 2).contains(&base) {
            continue;
        }

        for seq in [
            1,
            128,
            16_384,
            2_097_152,
            268_435_456,
            34_359_738_368,
            4_398_046_511_104,
            562_949_953_421_312,
            72_057_594_037_927_936,
            u64::MAX,
        ] {
            i.sequence = Some(kami_core::SenderSequence::new(seq));
            for ts in [
                0,
                128,
                16_384,
                2_097_152,
                268_435_456,
                34_359_738_368,
                4_398_046_511_104,
            ] {
                i.created_at_ms = ts;
                let mut clone = Session::from_pickle(s.pickle());
                let ct = encode_ciphertext(&clone.encrypt(i.encode().unwrap()).unwrap());
                let frame = unbounded(&i, &ct);
                if frame.len() == target {
                    return (i, frame);
                }
            }
        }
        i.sequence = Some(kami_core::SenderSequence::new(1));
        i.created_at_ms = 1000;
    }
    panic!("no exact {target}-byte provider frame found")
}
#[test]
fn exact_65536_and_one_over_include_actual_prekey_ciphertext_and_protobuf_overhead() {
    let p = pair();
    let raw = raw(&p);
    let (i, frame) = exact_frame(&p, &raw, 65_536);
    assert_eq!(frame.len(), 65_536);
    assert!(inbound(&p, &frame, &i).is_ok());
    let s = outbound(&p);
    let mut local = i.clone();
    local.session_id = s.session_id();
    assert_eq!(send(&p, &s, &local).frame.len(), 65_536);
    let (over, frame) = exact_frame(&p, &raw, 65_537);
    assert_eq!(frame.len(), 65_537);
    assert!(inbound(&p, &frame, &over).is_err());
    let mut local = over.clone();
    local.session_id = s.session_id();
    assert!(s
        .stage_send(&local, Some(&p.trust_b), Some(&local.recipient_peer), 1000)
        .is_err());
    // Failed oversize send leaves source session usable at the exact limit.
    let mut local = i.clone();
    local.session_id = s.session_id();
    assert_eq!(send(&p, &s, &local).frame.len(), 65_536);
}
#[test]
fn exact_normal_frame_limit_and_one_over_are_enforced_after_handshake() {
    let p = pair();
    let mut raw = raw(&p);
    let template = outbound(&p);
    let mut first = inner_for(&p, &template);
    first.session_id = raw.session_id();
    let message = raw.encrypt(first.encode().unwrap()).unwrap();
    let ct = encode_ciphertext(&message);
    let first_frame = outer(&first, &ct, &first.commitment().unwrap());
    let inbound = inbound(&p, &first_frame, &first).unwrap();
    let mut reply = first.clone();
    std::mem::swap(&mut reply.context.sender, &mut reply.context.recipient);
    std::mem::swap(&mut reply.sender_peer, &mut reply.recipient_peer);
    std::mem::swap(&mut reply.sender_binding, &mut reply.recipient_binding);
    reply.direction = Direction::ResponderToInitiator;
    let sent = inbound
        .session
        .stage_send(&reply, Some(&p.trust_a), Some(&reply.recipient_peer), 1000)
        .unwrap();
    let frame = kami_protocol::decode_frame(&sent.frame).unwrap();
    let payload = frame.protected_payload();
    let reply =
        vodozemac::olm::OlmMessage::from_parts(usize::from(payload[0]), &payload[1..]).unwrap();
    raw.decrypt(&reply).unwrap();
    let bob = sent.session;
    let (i, frame) = exact_frame(&p, &raw, 65_536);
    assert_eq!(frame.len(), 65_536);
    let mut sending = i.clone();
    std::mem::swap(&mut sending.context.sender, &mut sending.context.recipient);
    std::mem::swap(&mut sending.sender_peer, &mut sending.recipient_peer);
    std::mem::swap(&mut sending.sender_binding, &mut sending.recipient_binding);
    sending.direction = Direction::ResponderToInitiator;
    assert_eq!(
        bob.stage_send(
            &sending,
            Some(&p.trust_a),
            Some(&sending.recipient_peer),
            1000
        )
        .unwrap()
        .frame
        .len(),
        65_536
    );
    assert!(bob
        .stage_receive(
            &frame,
            &expected(&i),
            Some(&p.trust_a),
            Some(&i.sender_peer),
            1000
        )
        .is_ok());
    let (over, frame) = exact_frame(&p, &raw, 65_537);
    assert_eq!(frame.len(), 65_537);
    sending.payload = over.payload.clone();
    sending.sequence = over.sequence;
    sending.created_at_ms = over.created_at_ms;
    assert!(bob
        .stage_send(
            &sending,
            Some(&p.trust_a),
            Some(&sending.recipient_peer),
            1000
        )
        .is_err());
    assert!(bob
        .stage_receive(
            &frame,
            &expected(&over),
            Some(&p.trust_a),
            Some(&over.sender_peer),
            1000
        )
        .is_err());
}
