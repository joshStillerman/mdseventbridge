use std::net::Ipv4Addr;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct MulticastConfig {
    first: [u8; 3],
    pub range_start: u8,
    pub range_end: u8,
}

impl MulticastConfig {
    pub fn parse(setting: &str) -> Result<Self> {
        let trimmed = setting.trim();
        if trimmed.eq_ignore_ascii_case("compat") {
            return Ok(Self {
                first: [225, 0, 0],
                range_start: 0,
                range_end: 255,
            });
        }

        let mut octets = trimmed.split('.');
        let p1 = parse_u8(octets.next(), "octet1")?;
        let p2 = parse_u8(octets.next(), "octet2")?;
        let p3 = parse_u8(octets.next(), "octet3")?;
        let p4_or_range = octets
            .next()
            .ok_or_else(|| anyhow!("missing 4th octet in address setting"))?;
        if octets.next().is_some() {
            return Err(anyhow!("too many octets in address setting"));
        }

        let (range_start, range_end) = if let Some((start, end)) = p4_or_range.split_once('-') {
            let s = parse_u8(Some(start), "range_start")?;
            let e = parse_u8(Some(end), "range_end")?;
            if e < s {
                return Err(anyhow!("range_end must be >= range_start"));
            }
            (s, e)
        } else {
            let p4 = parse_u8(Some(p4_or_range), "octet4")?;
            (p4, p4)
        };

        Ok(Self {
            first: [p1, p2, p3],
            range_start,
            range_end,
        })
    }

    #[must_use]
    pub fn ip_for_index(&self, index: u8) -> Ipv4Addr {
        Ipv4Addr::new(self.first[0], self.first[1], self.first[2], index)
    }

    #[must_use]
    pub fn event_to_multicast(&self, event_name: &str) -> Ipv4Addr {
        let hash: u32 = event_name.bytes().map(u32::from).sum();
        let span = f32::from(self.range_end - self.range_start + 1);
        let mapped = f32::from(self.range_start) + ((hash % 256) as f32 / 256.0) * span;
        let index = mapped as u8;
        self.ip_for_index(index)
    }
}

fn parse_u8(part: Option<&str>, label: &str) -> Result<u8> {
    let text = part.ok_or_else(|| anyhow!("missing {label}"))?;
    let value: u16 = text
        .parse()
        .map_err(|_| anyhow!("invalid {label}: expected integer"))?;
    u8::try_from(value).map_err(|_| anyhow!("invalid {label}: out of [0,255]"))
}
