use kami_protocol::prekey_exchange::{decode, encode, Exchange, Query, MAX_FRAME_BYTES};

fn query() -> Exchange {
    Exchange::Query(Query {
        request: [1; 32],
        requester: [2; 32],
        publisher: [3; 32],
        minimum_generation: 1,
    })
}

#[test]
fn all_kinds_are_canonical_bounded_and_domain_separated() {
    use kami_protocol::prekey_exchange::{
        Acknowledgement, Failure, Selection, MAX_BODY_BYTES, MAX_PROOF_BYTES,
    };
    let ack = Acknowledgement {
        request: [1; 32],
        sender: [2; 32],
        recipient: [3; 32],
        bundle_generation: 1,
        response_digest: [4; 32],
    };
    let selection = Selection {
        request: [1; 32],
        publisher: [3; 32],
        recipient: [2; 32],
        account_generation: 1,
        bundle_generation: 1,
        proof: vec![5; MAX_PROOF_BYTES],
    };
    for value in [
        query(),
        Exchange::Selection(selection.clone()),
        Exchange::Ack(ack.clone()),
        Exchange::AckResult(ack),
        Exchange::Error {
            request: [1; 32],
            failure: Failure::Indeterminate,
        },
    ] {
        let bytes = encode(&value).unwrap();
        assert!(bytes.len() + 4 <= MAX_FRAME_BYTES);
        assert_eq!(decode(&bytes).unwrap(), value);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end]).is_err());
        }
        for i in 0..bytes.len() {
            let mut changed = bytes.clone();
            changed[i] ^= 128;
            if let Ok(parsed) = decode(&changed) {
                assert_eq!(encode(&parsed).unwrap(), changed);
            }
        }
        for suffix in [&[8, 1][..], &[80, 1][..], &[0][..]] {
            let mut changed = bytes.clone();
            changed.extend(suffix);
            assert!(decode(&changed).is_err());
        }
    }
    let mut oversized = selection;
    oversized.proof.push(0);
    assert!(encode(&Exchange::Selection(oversized)).is_err());
    assert!(decode(&vec![0; MAX_BODY_BYTES + 1]).is_err());
    for bad in [0, u64::MAX] {
        let mut q = match query() {
            Exchange::Query(q) => q,
            _ => unreachable!(),
        };
        q.minimum_generation = bad;
        assert!(encode(&Exchange::Query(q)).is_err());
    }
}
#[test]
fn canonical_query_and_strict_rejection() {
    let bytes = encode(&query()).unwrap();
    assert_eq!(decode(&bytes).unwrap(), query());
    // Canonical protobuf: version, kind, challenge, requester, publisher, floor.
    let mut golden = vec![8, 1, 16, 1, 26, 32];
    golden.extend([1; 32]);
    golden.extend([34, 32]);
    golden.extend([2; 32]);
    golden.extend([42, 32]);
    golden.extend([3; 32]);
    golden.extend([48, 1]);
    assert_eq!(bytes, golden);
    for end in 0..bytes.len() {
        assert!(decode(&bytes[..end]).is_err());
    }
    for suffix in [&[8, 1][..], &[64, 1][..], &[0][..]] {
        let mut bad = bytes.clone();
        bad.extend(suffix);
        assert!(decode(&bad).is_err());
    }
    let mut nonminimal = bytes.clone();
    nonminimal.splice(1..2, [129, 0]);
    assert!(decode(&nonminimal).is_err());
    assert!(decode(&vec![0; MAX_FRAME_BYTES]).is_err());
}
