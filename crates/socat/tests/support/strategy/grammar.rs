use super::*;

fn chars(
    set: &'static str,
    len: std::ops::RangeInclusive<usize>,
) -> impl Strategy<Value = String> {
    let set: Vec<char> = set.chars().collect();
    prop::collection::vec(prop::sample::select(set), len)
        .prop_map(|v| v.into_iter().collect())
}

const ALNUM: &str = "abcdefghijklmnopqrstuvwxyz0123456789";

// https://www.rfc-editor.org/rfc/rfc1123#section-2.1
fn label(alnum: &'static str) -> impl Strategy<Value = String> {
    (
        chars(alnum, 1..=1),
        proptest::option::of((
            prop_oneof![
                9 => chars("abcdefghijklmnopqrstuvwxyz0123456789-", 0..=6),
                1 => chars("abcdefghijklmnopqrstuvwxyz0123456789-", 55..=62),
            ],
            chars(alnum, 1..=1),
        )),
    )
        .prop_map(|(a, b)| match b {
            Some((mid, end)) => format!("{a}{mid}{end}"),
            None => a,
        })
}

pub(crate) fn name() -> impl Strategy<Value = Name> {
    let ascii = prop::collection::vec(label(ALNUM), 1..=4);
    #[cfg(feature = "idn")]
    let labels =
        prop_oneof![ascii, prop::collection::vec(label("abcéöü"), 1..=3)];
    #[cfg(not(feature = "idn"))]
    let labels = ascii;
    (labels, any::<bool>())
        .prop_map(|(l, root)| {
            format!("{}{}", l.join("."), if root { "." } else { "" })
        })
        .prop_filter_map("not a name", |n| Name::new(&n))
}

// https://www.rfc-editor.org/rfc/rfc9844#section-6
pub(crate) fn zone() -> impl Strategy<Value = Zone> {
    chars("abcdefghijklmnopqrstuvwxyzABCZ0123456789._-", 1..=15)
        .prop_map(|z| Zone::new(&z).expect("zone"))
}

pub(crate) fn host() -> impl Strategy<Value = Host> {
    prop_oneof![
        name().prop_map(Host::Name),
        any::<u32>().prop_map(|v| Host::V4(Ipv4Addr::from(v))),
        (any::<u128>(), proptest::option::of(zone()))
            .prop_map(|(v, z)| Host::V6(Ipv6Addr::from(v), z)),
    ]
}

// https://www.rfc-editor.org/rfc/rfc6335#section-5.1
pub(crate) fn service() -> impl Strategy<Value = Service> {
    prop::collection::vec(chars(ALNUM, 1..=4), 1..=3)
        .prop_map(|runs| runs.join("-"))
        .prop_filter_map("not a service", |s| Service::new(&s))
}

pub(crate) fn port() -> impl Strategy<Value = Port> {
    prop_oneof![
        any::<u16>().prop_map(Port::Number),
        service().prop_map(Port::Service),
    ]
}

// https://www.rfc-editor.org/rfc/rfc3986#section-3.3
fn pchar() -> impl Strategy<Value = String> {
    prop_oneof![
        chars("abcXYZ019-._~!$&'()*+,;=:@", 1..=1),
        (0u8..=255).prop_map(|b| format!("%{b:02X}")),
    ]
}

// https://www.rfc-editor.org/rfc/rfc6455#section-3
pub(crate) fn resource() -> impl Strategy<Value = Resource> {
    let segment =
        prop::collection::vec(pchar(), 0..=4).prop_map(|v| v.concat());
    let query = prop::collection::vec(
        prop_oneof![pchar(), Just("/".to_owned()), Just("?".to_owned())],
        0..=4,
    )
    .prop_map(|v| v.concat());
    prop_oneof![
        Just(Resource::default()),
        (
            prop::collection::vec(segment, 1..=3),
            proptest::option::of(query)
        )
            .prop_map(|(segs, q)| {
                let path: String =
                    segs.iter().map(|s| format!("/{s}")).collect();
                q.map_or(path.clone(), |q| format!("{path}?{q}"))
            })
            .prop_map(|r| Resource::new(&r).expect("resource")),
    ]
}
