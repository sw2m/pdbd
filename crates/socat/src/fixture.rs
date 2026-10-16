use crate::{
    Address, Error, Host, Inet, Kind, Name, Net, Opt, Options, Port, Resource,
    Service, Target, Text, Unchecked, Web,
};
use std::num::NonZeroU32;

pub(crate) fn parse(s: &str) -> Address {
    Address::parse(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

pub(crate) fn fail(s: &str) -> Error {
    match Address::parse(s) {
        Ok(a) => panic!("{s:?} parsed as {a:?}"),
        Err(e) => e,
    }
}

pub(crate) fn text(s: &str) -> Text {
    Text::new(s).expect("non-empty text")
}

pub(crate) fn name(n: &str) -> Host {
    Host::Name(Name::new(n).expect("valid host name"))
}

pub(crate) fn num(p: u16) -> Port {
    Port::Number(p)
}

pub(crate) fn svc(s: &str) -> Port {
    Port::Service(Service::new(s).expect("valid service"))
}

pub(crate) fn opt(n: &str, v: Option<&str>) -> Opt {
    Opt::new(n, v).expect("valid option")
}

pub(crate) fn res(r: &str) -> Resource {
    Resource::new(r).expect("valid resource")
}

pub(crate) fn address(
    target: Target,
    mux: bool,
    cap: u32,
    rest: Vec<Opt>,
) -> Address {
    let options = Options::new(mux, NonZeroU32::new(cap), Unchecked::new(rest))
        .expect("valid options");
    Address::new(target, options).expect("valid address")
}

pub(crate) fn plain(target: Target, rest: Vec<Opt>) -> Address {
    address(target, false, 0, rest)
}

pub(crate) fn tcp(host: Host, port: Port) -> Target {
    Target::Net {
        net: Net::Tcp,
        inet: Inet::Connect { host, port },
    }
}

pub(crate) fn web(web: Web, inet: Inet, resource: &str) -> Target {
    Target::Web {
        web,
        inet,
        resource: res(resource),
    }
}

pub(crate) fn exec(s: &str) -> Target {
    parse(s).target().clone()
}

pub(crate) fn table(cases: &[(&str, usize, Kind)]) {
    for (s, at, kind) in cases {
        assert_eq!(fail(s), Error::new(*at, kind.clone()), "{s:?}");
    }
}

pub(crate) fn same(cases: &[(&str, &str)]) {
    for (s, canonical) in cases {
        assert_eq!(parse(s), parse(canonical), "{s}");
        assert_eq!(parse(s).to_string(), *canonical, "{s}");
    }
}
