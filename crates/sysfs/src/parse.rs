//! Pure parsing helpers for sysfs/procfs files. All functions are total:
//! malformed input yields `None` (or a partial result) and never panics, so
//! the provider can degrade per-field instead of failing a whole section.

use std::net::{Ipv4Addr, Ipv6Addr};

/// IFF_UP bit in the sysfs `flags` file (hex value).
pub const IFF_UP: u32 = 0x1;

/// Parse a hex u32 with or without a `0x` prefix (sysfs `flags`).
pub fn parse_hex_u32(raw: &str) -> Option<u32> {
    let t = raw.trim();
    let t = t
        .strip_prefix("0x")
        .or_else(|| t.strip_prefix("0X"))
        .unwrap_or(t);
    u32::from_str_radix(t, 16).ok()
}

/// Parse a decimal u32 (sysfs `ifindex`, `mtu`, `tx_queue_len`, `type`).
pub fn parse_dec_u32(raw: &str) -> Option<u32> {
    raw.trim().parse().ok()
}

/// `/proc/net/route` stores IPv4 values as little-endian hex: the gateway
/// `0100A8C0` is 192.168.0.1. Verified against a live kernel.
pub fn parse_le_hex_ipv4(hex: &str) -> Option<Ipv4Addr> {
    let hex = hex.trim();
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (i, chunk) in hex.as_bytes().chunks(2).enumerate() {
        bytes[i] = u8::from_str_radix(std::str::from_utf8(chunk).ok()?, 16).ok()?;
    }
    bytes.reverse();
    Some(Ipv4Addr::from(bytes))
}

/// `/proc/net/if_inet6` stores IPv6 addresses as plain big-endian hex.
pub fn parse_hex_ipv6(hex: &str) -> Option<Ipv6Addr> {
    let hex = hex.trim();
    if hex.len() != 32 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u128::from_str_radix(hex, 16).ok()?;
    Some(Ipv6Addr::from(value))
}

/// One `/proc/net/if_inet6` line:
/// `address ifindex(hex) prefix(hex) scope(hex) flags(hex) name`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IfInet6Entry {
    pub address: Ipv6Addr,
    pub ifindex: u32,
    pub prefix_len: u8,
    pub scope_raw: u8,
    pub flags_raw: u8,
    pub name: String,
}

pub fn parse_if_inet6_line(line: &str) -> Option<IfInet6Entry> {
    let mut fields = line.split_whitespace();
    let address = parse_hex_ipv6(fields.next()?)?;
    let ifindex = u32::from_str_radix(fields.next()?, 16).ok()?;
    let prefix_len = u8::from_str_radix(fields.next()?, 16).ok()?;
    let scope_raw = u8::from_str_radix(fields.next()?, 16).ok()?;
    let flags_raw = u8::from_str_radix(fields.next()?, 16).ok()?;
    let name = fields.next()?.to_string();
    if fields.next().is_some() {
        return None;
    }
    Some(IfInet6Entry {
        address,
        ifindex,
        prefix_len,
        scope_raw,
        flags_raw,
        name,
    })
}

/// IFA_F flag bits as reported in `/proc/net/if_inet6`.
pub fn if_inet6_flags(raw: u8) -> Vec<String> {
    let mut flags = Vec::new();
    if raw & 0x80 != 0 {
        flags.push("permanent".into());
    }
    if raw & 0x40 != 0 {
        flags.push("tentative".into());
    }
    if raw & 0x20 != 0 {
        flags.push("deprecated".into());
    }
    if raw & 0x10 != 0 {
        flags.push("homeaddress".into());
    }
    if raw & 0x04 != 0 {
        flags.push("optimistic".into());
    }
    if raw & 0x01 != 0 {
        flags.push("secondary".into());
    }
    flags
}

/// One `/proc/net/route` line (the header line must be excluded by the
/// caller). Columns: iface destination gateway flags refcnt use metric mask
/// mtu window irtt — destination/gateway/mask in little-endian hex.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteEntry {
    pub iface: String,
    pub destination: Ipv4Addr,
    pub mask: Ipv4Addr,
    pub gateway: Option<Ipv4Addr>,
    pub metric: u32,
}

/// True for the `/proc/net/route` header line.
pub fn is_route_header(line: &str) -> bool {
    line.trim_start().starts_with("Iface")
}

pub fn parse_route_line(line: &str) -> Option<RouteEntry> {
    let mut fields = line.split_whitespace();
    let iface = fields.next()?.to_string();
    let destination = parse_le_hex_ipv4(fields.next()?)?;
    let gateway_raw = parse_le_hex_ipv4(fields.next()?)?;
    let _flags = fields.next()?;
    let _refcnt = fields.next()?;
    let _use = fields.next()?;
    let metric = fields.next()?.parse().ok()?;
    let mask = parse_le_hex_ipv4(fields.next()?)?;
    if iface.is_empty() {
        return None;
    }
    Some(RouteEntry {
        iface,
        destination,
        mask,
        gateway: (gateway_raw != Ipv4Addr::UNSPECIFIED).then_some(gateway_raw),
        metric,
    })
}

/// Prefix length from a `/proc/net/route` netmask.
pub fn prefix_len_from_mask(mask: Ipv4Addr) -> u8 {
    u32::from(mask).leading_ones() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn little_endian_route_values() {
        assert_eq!(
            parse_le_hex_ipv4("0100A8C0"),
            Some("192.168.0.1".parse().unwrap())
        );
        assert_eq!(
            parse_le_hex_ipv4("0000A8C0"),
            Some("192.168.0.0".parse().unwrap())
        );
        assert_eq!(
            parse_le_hex_ipv4("0000007F"),
            Some("127.0.0.0".parse().unwrap())
        );
        assert_eq!(
            parse_le_hex_ipv4("00000000"),
            Some("0.0.0.0".parse().unwrap())
        );
        assert_eq!(parse_le_hex_ipv4("nothex0"), None);
        assert_eq!(parse_le_hex_ipv4("123"), None);
    }

    #[test]
    fn if_inet6_line_roundtrip() {
        let entry = parse_if_inet6_line("fe8000000000000020d530fffeac0034 03 40 20 80     eth0")
            .expect("valid line");
        assert_eq!(entry.address.to_string(), "fe80::20d5:30ff:feac:34");
        assert_eq!(entry.ifindex, 3);
        assert_eq!(entry.prefix_len, 64);
        assert_eq!(entry.scope_raw, 0x20);
        assert_eq!(entry.flags_raw, 0x80);
        assert_eq!(entry.name, "eth0");
        assert_eq!(if_inet6_flags(0x80), vec!["permanent".to_string()]);
        assert!(if_inet6_flags(0x00).is_empty());
    }

    #[test]
    fn route_line_parsing() {
        assert!(is_route_header(
            "Iface        Destination     Gateway Flags   RefCnt  Use"
        ));
        let entry =
            parse_route_line("eth0    00000000        0100A8C0        0003    0       0       600     00000000        0       0       0")
                .expect("valid route line");
        assert_eq!(entry.iface, "eth0");
        assert_eq!(
            entry.destination,
            "0.0.0.0".parse::<std::net::Ipv4Addr>().unwrap()
        );
        assert_eq!(entry.gateway, Some("192.168.0.1".parse().unwrap()));
        assert_eq!(entry.metric, 600);
        assert_eq!(prefix_len_from_mask(entry.mask), 0);

        let connected =
            parse_route_line("eth0    0000A8C0        00000000        0001    0       0       0       00FFFFFF        0       0       0")
                .expect("valid route line");
        assert_eq!(connected.gateway, None, "zero gateway means on-link");
        assert_eq!(prefix_len_from_mask(connected.mask), 24);
        assert_eq!(
            connected.destination,
            "192.168.0.0".parse::<Ipv4Addr>().unwrap()
        );
    }

    #[test]
    fn flags_and_numbers() {
        assert_eq!(parse_hex_u32("0x10049"), Some(0x10049));
        assert_eq!(parse_hex_u32("0X1003"), Some(0x1003));
        assert_eq!(parse_hex_u32("1003"), Some(0x1003));
        assert_eq!(parse_dec_u32(" 42\n"), Some(42));
        assert_eq!(parse_dec_u32("-1"), None);
        assert_eq!(
            parse_hex_ipv6("00000000000000000000000000000001").map(|a| a.to_string()),
            Some("::1".to_string())
        );
    }
}
