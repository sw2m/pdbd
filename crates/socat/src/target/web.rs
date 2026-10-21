use crate::lex::{Params, Token};
use crate::{
    Error, Host, Inet, Kind, Mode, Name, Part, Port, Resource, Target,
};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Web {
    Ws,
    Wss,
    Webtransport,
}

impl Web {
    fn scheme(self) -> &'static str {
        match self {
            Self::Ws => "ws",
            Self::Wss => "wss",
            Self::Webtransport => "https",
        }
    }

    fn port(self) -> u16 {
        match self {
            Self::Ws => 80,
            Self::Wss | Self::Webtransport => 443,
        }
    }
}

impl Target {
    pub(crate) fn web(
        web: Web,
        params: &Params,
        mode: Mode,
    ) -> Result<Self, Error> {
        let tokens = params.tokens();
        if let [first, _, ..] = tokens {
            if mode == Mode::Listen && first.text.contains('/') {
                return Err(first.fail(Kind::Port(first.text.clone())));
            }
        }
        let bare = mode == Mode::Listen && tokens.len() == 1;
        let k = if bare { 0 } else { 1 };
        let split = tokens.get(k).and_then(|t| Some((t, t.text.find('/')?)));
        let Some((tok, i)) = split else {
            let inet = Inet::parse(params, mode)?;
            return Ok(Self::Web {
                web,
                inet,
                resource: Resource::default(),
            });
        };
        let mut path = tok.text[i..].to_string();
        for t in &tokens[k + 1..] {
            path.push(':');
            path.push_str(&t.text);
        }
        let resource = Resource::parse(&path, tok.at + i)?;
        let mut head = tokens[..k].to_vec();
        head.push(Token::new(&tok.text[..i], tok.at));
        let inet = Inet::parse(&Params::new(&head, params.end()), mode)?;
        Ok(Self::Web {
            web,
            inet,
            resource,
        })
    }

    // https://www.rfc-editor.org/rfc/rfc3986#section-3
    // https://www.rfc-editor.org/rfc/rfc6455#section-3
    // https://www.rfc-editor.org/rfc/rfc9110#section-4.2.2
    // https://www.rfc-editor.org/rfc/rfc9844#section-3
    pub(crate) fn url(web: Web, params: &Params) -> Result<Self, Error> {
        let joined = params.joined();
        let start = params.tokens()[0].at;
        let body = &joined[2..];
        let authority =
            &body[..body.find(['/', '?', '#']).unwrap_or(body.len())];
        let at = start + 2;
        let whole = || Error::new(at, Kind::Host(authority.into()));
        if authority.contains('@') {
            return Err(whole());
        }
        let split = match authority.starts_with('[') {
            true => authority.find(']').map(|i| i + 1).ok_or_else(whole)?,
            false => authority.find(':').unwrap_or(authority.len()),
        };
        let (host, port) = authority.split_at(split);
        let rest = &body[authority.len()..];
        if host.is_empty() {
            return Err(Error::new(at, Kind::Missing(Part::Host)));
        }
        let bad = || Error::new(at, Kind::Host(host.into()));
        let digits = port.strip_prefix(':').unwrap_or(port);
        let colon = usize::from(digits.len() != port.len());
        let wrong =
            || Error::new(at + split + colon, Kind::Port(digits.into()));
        let resource =
            |i: usize| Error::new(start + i, Kind::Resource(rest.into()));
        let text = format!("{}:{joined}", web.scheme());
        let uri =
            fluent_uri::Uri::parse(text.as_str()).map_err(|e| {
                match e.index() - web.scheme().len() - 1 {
                    i if i < 2 + split => bad(),
                    i if i < 2 + authority.len() => wrong(),
                    i => resource(i),
                }
            })?;
        if uri.fragment().is_some() {
            return Err(resource(
                2 + authority.len()
                    + rest.find('#').expect("a fragment has `#`"),
            ));
        }
        let parts = uri.authority().expect("the text has `//`");
        let host = match parts.host_parsed() {
            fluent_uri::component::Host::Ipv4(ip) => Host::V4(ip),
            fluent_uri::component::Host::Ipv6(ip) => Host::V6(ip, None),
            fluent_uri::component::Host::RegName(name) => {
                let decoded = Self::decode(name.decode()).ok_or_else(bad)?;
                Host::Name(Name::new(&decoded).ok_or_else(bad)?)
            }
            _ => return Err(bad()),
        };
        let port = match digits {
            "" => Port::Number(web.port()),
            d => Port::Number(d.parse().map_err(|_| wrong())?),
        };
        let path = match uri.path().as_str() {
            "" => "/",
            p => p,
        };
        let resource = match uri.query() {
            Some(q) => format!("{path}?{}", q.as_str()),
            None => path.to_string(),
        };
        Ok(Self::Web {
            web,
            inet: Inet::Connect { host, port },
            resource: Resource::new(&resource).expect("parsed as a URI"),
        })
    }

    // https://www.rfc-editor.org/rfc/rfc3986#section-3.2.2
    fn decode(chunks: fluent_uri::pct_enc::Decode) -> Option<String> {
        use fluent_uri::pct_enc::DecodedChunk;
        let mut bytes = Vec::new();
        for chunk in chunks {
            match chunk {
                DecodedChunk::Unencoded(s) => bytes.extend(s.bytes()),
                DecodedChunk::PctDecoded(b) => bytes.push(b),
            }
        }
        String::from_utf8(bytes).ok()
    }
}

impl fmt::Display for Web {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ws => "WS",
            Self::Wss => "WSS",
            Self::Webtransport => "WEBTRANSPORT",
        })
    }
}
