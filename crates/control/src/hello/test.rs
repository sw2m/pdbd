use super::*;
use proptest::prelude::*;

fn offer(
    implementation: &str,
    wire: u32,
    floor: u32,
    features: &[&str],
) -> Offer {
    Offer {
        implementation: implementation.into(),
        wire,
        floor,
        features: features.iter().map(|f| f.to_string()).collect(),
    }
}

fn request(implementation: &str, wire: u32, features: &[&str]) -> HelloRequest {
    HelloRequest {
        implementation: implementation.into(),
        wire_version: wire,
        features: features.iter().map(|f| f.to_string()).collect(),
    }
}

fn set(features: &[&str]) -> BTreeSet<String> {
    features.iter().map(|f| f.to_string()).collect()
}

// #14
#[test]
fn new_speaks_this_revision() {
    let o = Offer::new("pdbd", ["pty", "socat"]);
    assert_eq!(o.wire, WIRE);
    assert_eq!(o.floor, WIRE);
    assert_eq!(o.features, set(&["pty", "socat"]));
    let r = o.request();
    assert_eq!(r.implementation, "pdbd");
    assert_eq!(r.wire_version, WIRE);
    assert_eq!(r.features, vec!["pty", "socat"]);
}

// #14
#[test]
fn identical_offers_agree_on_everything() {
    let o = offer("pdbd", 1, 1, &["pty", "socat"]);
    let a = o.answer(&o.request()).unwrap();
    assert_eq!(
        a,
        Agreement {
            implementation: "pdbd".into(),
            wire: 1,
            features: set(&["pty", "socat"]),
        }
    );
}

// #14
#[test]
fn newer_peer_degrades_to_our_revision() {
    let a = offer("pdbd", 1, 1, &[])
        .answer(&request("gopdb", 7, &[]))
        .unwrap();
    assert_eq!(a.wire, 1);
    assert_eq!(a.implementation, "gopdb");
}

// #14
#[test]
fn older_peer_within_floor_sets_the_revision() {
    let a = offer("pdbd", 3, 2, &[])
        .answer(&request("jpdb", 2, &[]))
        .unwrap();
    assert_eq!(a.wire, 2);
}

// #14
#[test]
fn older_peer_below_floor_is_refused() {
    assert_eq!(
        offer("pdbd", 3, 2, &[]).answer(&request("cpdb", 1, &[])),
        Err(Refusal::Version {
            wire: 1,
            range: 2..=3
        })
    );
}

// #14
#[test]
fn unset_version_is_refused() {
    assert_eq!(
        offer("pdbd", 1, 1, &[]).answer(&request("gopdb", 0, &[])),
        Err(Refusal::Version {
            wire: 0,
            range: 1..=1
        })
    );
}

// #14
#[test]
fn unknown_features_are_dropped() {
    let a = offer("pdbd", 1, 1, &["pty"])
        .answer(&request("gopdb", 1, &["x-quic", "pty"]))
        .unwrap();
    assert_eq!(a.features, set(&["pty"]));
}

// #14
#[test]
fn overlapping_features_intersect() {
    let a = offer("pdbd", 1, 1, &["bind", "pty", "socat"])
        .answer(&request("gopdb", 1, &["socat", "pty", "x-mux"]))
        .unwrap();
    assert_eq!(a.features, set(&["pty", "socat"]));
}

// #14
#[test]
fn disjoint_features_agree_on_none() {
    let a = offer("pdbd", 1, 1, &["pty"])
        .answer(&request("gopdb", 1, &["socat"]))
        .unwrap();
    assert!(a.features.is_empty());
}

// #14
#[test]
fn duplicate_features_collapse() {
    let a = offer("pdbd", 1, 1, &["pty"])
        .answer(&request("gopdb", 1, &["pty", "pty"]))
        .unwrap();
    assert_eq!(a.features, set(&["pty"]));
}

// #14
#[test]
fn empty_implementation_is_refused() {
    assert_eq!(
        offer("pdbd", 1, 1, &[]).answer(&request("", 1, &[])),
        Err(Refusal::Implementation)
    );
}

// #14
#[test]
fn empty_feature_token_is_refused() {
    assert_eq!(
        offer("pdbd", 1, 1, &["pty"]).answer(&request(
            "gopdb",
            1,
            &["pty", ""]
        )),
        Err(Refusal::Token)
    );
}

// #14
#[test]
fn response_carries_our_id_and_the_agreement() {
    let o = offer("pdbd", 1, 1, &["pty", "socat"]);
    let a = o.answer(&request("gopdb", 4, &["socat", "pty"])).unwrap();
    let r = o.response(&a);
    assert_eq!(r.implementation, "pdbd");
    assert_eq!(r.wire_version, 1);
    assert_eq!(r.features, vec!["pty", "socat"]);
}

fn response(
    implementation: &str,
    wire: u32,
    features: &[&str],
) -> HelloResponse {
    HelloResponse {
        implementation: implementation.into(),
        wire_version: wire,
        features: features.iter().map(|f| f.to_string()).collect(),
    }
}

// #14
#[test]
fn accept_takes_a_subset_answer() {
    let a = offer("gopdb", 2, 1, &["pty", "socat"])
        .accept(&response("pdbd", 1, &["pty"]))
        .unwrap();
    assert_eq!(
        a,
        Agreement {
            implementation: "pdbd".into(),
            wire: 1,
            features: set(&["pty"]),
        }
    );
}

// #14
#[test]
fn accept_refuses_an_unoffered_feature() {
    assert_eq!(
        offer("pdb", 1, 1, &["pty"]).accept(&response(
            "pdbd",
            1,
            &["pty", "socat"]
        )),
        Err(Refusal::Unoffered("socat".into()))
    );
}

// #14
#[test]
fn accept_refuses_a_revision_above_ours() {
    assert_eq!(
        offer("pdb", 1, 1, &[]).accept(&response("pdbd", 2, &[])),
        Err(Refusal::Version {
            wire: 2,
            range: 1..=1
        })
    );
}

// #14
#[test]
fn accept_refuses_a_revision_below_our_floor() {
    assert_eq!(
        offer("pdb", 3, 2, &[]).accept(&response("pdbd", 1, &[])),
        Err(Refusal::Version {
            wire: 1,
            range: 2..=3
        })
    );
}

// #14
#[test]
fn accept_refuses_an_anonymous_answer() {
    assert_eq!(
        offer("pdb", 1, 1, &[]).accept(&response("", 1, &[])),
        Err(Refusal::Implementation)
    );
}

// #14
#[test]
fn accept_refuses_an_empty_token() {
    assert_eq!(
        offer("pdb", 1, 1, &["pty"]).accept(&response("pdbd", 1, &[""])),
        Err(Refusal::Token)
    );
}

fn token() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["", "pty", "socat", "bind", "x-a", "x-b"])
        .prop_map(String::from)
}

fn id() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["", "pdbd", "gopdb", "jpdb"])
        .prop_map(String::from)
}

fn arbitrary() -> impl Strategy<Value = Offer> {
    (
        id(),
        0u32..5,
        0u32..5,
        prop::collection::btree_set(token(), 0..5),
    )
        .prop_map(|(implementation, a, b, features)| Offer {
            implementation,
            wire: a.max(b),
            floor: a.min(b).max(1),
            features,
        })
}

fn valid() -> impl Strategy<Value = Offer> {
    arbitrary().prop_filter("a well-formed offer", |o| {
        !o.implementation.is_empty()
            && o.floor <= o.wire
            && !o.features.contains("")
    })
}

fn wire() -> impl Strategy<Value = HelloRequest> {
    (id(), 0u32..6, prop::collection::vec(token(), 0..6)).prop_map(
        |(implementation, wire_version, features)| HelloRequest {
            implementation,
            wire_version,
            features,
        },
    )
}

proptest! {
    // #14
    #[test]
    fn answer_matches_the_reference(o in valid(), r in wire()) {
        let theirs: BTreeSet<String> = r.features.iter().cloned().collect();
        let expected = if r.implementation.is_empty() {
            Err(Refusal::Implementation)
        } else if theirs.contains("") {
            Err(Refusal::Token)
        } else if r.wire_version < o.floor {
            Err(Refusal::Version { wire: r.wire_version, range: o.floor..=o.wire })
        } else {
            Ok(Agreement {
                implementation: r.implementation.clone(),
                wire: o.wire.min(r.wire_version),
                features: o.features.intersection(&theirs).cloned().collect(),
            })
        };
        prop_assert_eq!(o.answer(&r), expected);
    }

    // #14
    #[test]
    fn agreement_is_symmetric(a in valid(), b in valid()) {
        if let (Ok(x), Ok(y)) = (a.answer(&b.request()), b.answer(&a.request())) {
            prop_assert_eq!(x.wire, y.wire);
            prop_assert_eq!(x.features, y.features);
        }
    }

    // #14
    #[test]
    fn handshake_round_trips(client in valid(), server in valid()) {
        match server.answer(&client.request()) {
            Ok(agreed) => {
                let seen = client.accept(&server.response(&agreed));
                let expected = if agreed.wire < client.floor {
                    Err(Refusal::Version { wire: agreed.wire, range: client.floor..=client.wire })
                } else {
                    Ok(Agreement {
                        implementation: server.implementation.clone(),
                        wire: agreed.wire,
                        features: agreed.features.clone(),
                    })
                };
                prop_assert_eq!(seen, expected);
                prop_assert!(agreed.features.is_subset(&client.features));
                prop_assert!(agreed.features.is_subset(&server.features));
            }
            Err(Refusal::Version { wire, .. }) => prop_assert!(wire < server.floor),
            Err(other) => prop_assert!(false, "valid offers refused: {:?}", other),
        }
    }

    // #14
    #[test]
    fn accept_never_widens_the_offer(o in valid(), r in wire()) {
        let answer = HelloResponse {
            implementation: r.implementation,
            wire_version: r.wire_version,
            features: r.features,
        };
        if let Ok(a) = o.accept(&answer) {
            prop_assert!(a.features.is_subset(&o.features));
            prop_assert!((o.floor..=o.wire).contains(&a.wire));
            prop_assert!(!a.implementation.is_empty());
        }
    }
}
