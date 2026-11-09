//! Session codes: the host's address (IPv4 and port) and the session's key as one Crockford
//! base32 number, what a host gives its players to join with.

use std::net::{Ipv4Addr, SocketAddrV4};

use super::link::{is_local, port};
use super::session;

/// A join code for an address and key (what `parse` reads back).
pub(crate) fn code_for(address: SocketAddrV4, key: u32) -> String {
    encode((u32::from(*address.ip()) as u128) << 48 | (address.port() as u128) << 32 | key as u128)
}

/// The session's join code: the host's address (IPv4 and port) and key, in Crockford base32 (a
/// host's, or the one this player joined with).
pub fn code() -> Option<String> {
    let s = session();
    let s = s.as_ref()?;
    let a = s.address?;
    Some(encode((u32::from(*a.ip()) as u128) << 48 | (a.port() as u128) << 32 | s.key as u128))
}

/// Whether the hosted session's code names an address on the host's own network (not one for
/// the internet).
pub fn code_is_local() -> bool {
    session().as_ref().and_then(|s| s.address).is_some_and(|a| is_local(*a.ip()))
}

/// The address a join code names, and the session's key in it. Also takes a plain "a.b.c.d" or
/// "a.b.c.d:port" (no key: for a host on the same network), and a code without a key.
pub fn parse(code: &str) -> Option<(SocketAddrV4, Option<u32>)> {
    if let Ok(a) = code.trim().parse::<SocketAddrV4>() {
        return Some((a, None));
    }
    if let Ok(ip) = code.trim().parse::<Ipv4Addr>() {
        return Some((SocketAddrV4::new(ip, port()), None));
    }
    let v = decode(code)?;
    if v >> 80 != 0 {
        return None;
    }
    if v >> 48 == 0 {
        return Some((SocketAddrV4::new(Ipv4Addr::from((v >> 16) as u32), v as u16), None));
    }
    Some((SocketAddrV4::new(Ipv4Addr::from((v >> 48) as u32), (v >> 32) as u16), Some(v as u32)))
}

const CODE_DIGITS: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn encode(mut v: u128) -> String {
    let mut out = Vec::new();
    loop {
        out.push(CODE_DIGITS[(v & 31) as usize]);
        v >>= 5;
        if v == 0 {
            break;
        }
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// Crockford base32 (case and O/I/L read leniently, '-' and spaces ignored).
fn decode(code: &str) -> Option<u128> {
    let mut v: u128 = 0;
    let mut digits = 0;
    for c in code.chars().filter(|c| !matches!(c, '-' | ' ')) {
        let c = match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            c => c,
        };
        let d = CODE_DIGITS.iter().position(|&x| x as char == c)? as u128;
        v = v.checked_mul(32)?.checked_add(d)?;
        digits += 1;
    }
    (digits > 0).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::p2p::link::PORT;

    #[test]
    fn codes_name_lan_addresses() {
        let a = SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 23), PORT);
        let c = encode((u32::from(*a.ip()) as u128) << 16 | a.port() as u128);
        assert_eq!(parse(&c).map(|(a, _)| a), Some(a));
        assert_eq!(parse(&c.to_lowercase()).map(|(a, _)| a), Some(a));
        let k = encode((u32::from(Ipv4Addr::new(203, 0, 113, 9)) as u128) << 48 | 27861u128 << 32 | 0xDEAD_BEEF);
        assert_eq!(parse(&k), Some((SocketAddrV4::new(Ipv4Addr::new(203, 0, 113, 9), 27861), Some(0xDEAD_BEEF))));
        assert_eq!(parse("192.168.1.23").map(|(a, _)| a), Some(a));
        assert_eq!(parse("10.0.0.2:5000").map(|(a, _)| a), Some(SocketAddrV4::new(Ipv4Addr::new(10, 0, 0, 2), 5000)));
        assert_eq!(parse("U").map(|(a, _)| a), None);
    }
}
