use mdsevent_tcp_bridge::multicast::MulticastConfig;
use mdsevent_tcp_bridge::wire::{decode_udp_event, encode_udp_event};

#[test]
fn multicast_mapping_is_stable() {
    let cfg = MulticastConfig::parse("239.10.10.10").expect("parse should succeed");
    let ip = cfg.event_to_multicast("TEST_EVENT");
    assert_eq!(ip.to_string(), "239.10.10.10");
}

#[test]
fn wire_decode_matches_encode() {
    let datagram = encode_udp_event("TEST", b"payload");
    let decoded = decode_udp_event(&datagram).expect("decode should succeed");
    assert_eq!(decoded.event_name, "TEST");
    assert_eq!(decoded.payload_len, 7);
}
