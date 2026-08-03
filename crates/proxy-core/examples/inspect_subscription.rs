use proxy_core::{parse_subscription, NodeRegion, ProxyProtocol, RejectionReason};
use std::collections::BTreeMap;
use std::env;
use std::fs;

fn main() {
    let Some(path) = env::args().nth(1) else {
        eprintln!("usage: inspect_subscription <subscription-file>");
        std::process::exit(2);
    };
    let payload = match fs::read(&path) {
        Ok(payload) => payload,
        Err(error) => {
            eprintln!("failed to read subscription: {error}");
            std::process::exit(1);
        }
    };
    let parsed = match parse_subscription(&payload) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("failed to parse subscription: {error}");
            std::process::exit(1);
        }
    };

    let mut regions = BTreeMap::new();
    let mut protocols = BTreeMap::new();
    let mut rejected = BTreeMap::new();
    for node in &parsed.candidates {
        *regions.entry(region_name(node.region)).or_insert(0_usize) += 1;
        *protocols
            .entry(protocol_name(node.protocol))
            .or_insert(0_usize) += 1;
    }
    for node in &parsed.rejected {
        *rejected
            .entry(rejection_name(node.reason))
            .or_insert(0_usize) += 1;
    }

    println!("activation_candidates={}", parsed.candidates.len());
    println!("regions={regions:?}");
    println!("protocols={protocols:?}");
    println!("rejected={rejected:?}");
    for node in &parsed.candidates {
        println!(
            "candidate index={} protocol={} region={} name={}",
            node.index,
            protocol_name(node.protocol),
            region_name(node.region),
            node.name
        );
    }
}

fn region_name(region: NodeRegion) -> &'static str {
    match region {
        NodeRegion::HongKong => "hong_kong",
        NodeRegion::MainlandChina => "mainland_china",
        NodeRegion::Taiwan => "taiwan",
        NodeRegion::Japan => "japan",
        NodeRegion::UnitedStates => "united_states",
        NodeRegion::Singapore => "singapore",
        NodeRegion::Netherlands => "netherlands",
        NodeRegion::France => "france",
        NodeRegion::Germany => "germany",
        NodeRegion::UnitedKingdom => "united_kingdom",
        NodeRegion::Brazil => "brazil",
        NodeRegion::Other => "other",
    }
}

fn protocol_name(protocol: ProxyProtocol) -> &'static str {
    match protocol {
        ProxyProtocol::Vless => "vless",
        ProxyProtocol::Hysteria2 => "hysteria2",
    }
}

fn rejection_name(reason: RejectionReason) -> &'static str {
    match reason {
        RejectionReason::Metadata => "metadata",
        RejectionReason::ExcludedRegion => "excluded_region",
        RejectionReason::UnsupportedProtocol => "unsupported_protocol",
        RejectionReason::InvalidNode => "invalid_node",
    }
}
