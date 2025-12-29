use aegis_node::threat_intel_p2p::{P2PConfig, ThreatIntelP2P, ThreatIntelligence};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::test]
async fn test_p2p_network_creation() {
    let config = P2PConfig::default();
    let p2p = ThreatIntelP2P::new(config);

    assert!(p2p.is_ok(), "Failed to create P2P network");

    let p2p = p2p.unwrap();
    let peer_id = p2p.peer_id();

    assert!(!peer_id.to_string().is_empty(), "Peer ID should not be empty");
}

#[tokio::test]
#[ignore = "Requires mDNS network discovery which is environment-dependent"]
async fn test_two_peers_communication() {
    // Create two P2P nodes
    let mut config1 = P2PConfig::default();
    config1.listen_port = 9101;

    let mut config2 = P2PConfig::default();
    config2.listen_port = 9102;

    let mut p2p1 = ThreatIntelP2P::new(config1.clone()).expect("Failed to create P2P node 1");
    let mut p2p2 = ThreatIntelP2P::new(config2.clone()).expect("Failed to create P2P node 2");

    // Start listening
    p2p1.listen(config1.listen_port).expect("Failed to listen on node 1");
    p2p2.listen(config2.listen_port).expect("Failed to listen on node 2");

    // Give them time to discover each other via mDNS
    sleep(Duration::from_secs(2)).await;

    // Create a test threat
    let threat = ThreatIntelligence::new(
        "10.0.0.100".to_string(),
        "test_threat".to_string(),
        8,
        300,
        "test-node-1".to_string(),
    );

    // Publish from node 1
    let publish_result = p2p1.publish(&threat);
    assert!(publish_result.is_ok(), "Failed to publish threat");

    // Note: In a real test, we would need to run the event loop to receive messages
    // This is a basic connectivity test
}

#[test]
fn test_threat_intelligence_validation() {
    // Valid threat
    let valid = ThreatIntelligence::new(
        "192.168.1.100".to_string(),
        "syn_flood".to_string(),
        7,
        600,
        "node-1".to_string(),
    );
    assert!(valid.validate().is_ok());

    // Invalid IP
    let invalid_ip = ThreatIntelligence::new(
        "999.999.999.999".to_string(),
        "syn_flood".to_string(),
        7,
        600,
        "node-1".to_string(),
    );
    assert!(invalid_ip.validate().is_err());

    // Invalid severity (too low)
    let invalid_severity_low = ThreatIntelligence::new(
        "192.168.1.100".to_string(),
        "syn_flood".to_string(),
        0,
        600,
        "node-1".to_string(),
    );
    assert!(invalid_severity_low.validate().is_err());

    // Invalid severity (too high)
    let invalid_severity_high = ThreatIntelligence::new(
        "192.168.1.100".to_string(),
        "syn_flood".to_string(),
        11,
        600,
        "node-1".to_string(),
    );
    assert!(invalid_severity_high.validate().is_err());

    // Invalid duration (0)
    let invalid_duration_zero = ThreatIntelligence::new(
        "192.168.1.100".to_string(),
        "syn_flood".to_string(),
        7,
        0,
        "node-1".to_string(),
    );
    assert!(invalid_duration_zero.validate().is_err());

    // Invalid duration (too long, >24 hours)
    let invalid_duration_long = ThreatIntelligence::new(
        "192.168.1.100".to_string(),
        "syn_flood".to_string(),
        7,
        90000,
        "node-1".to_string(),
    );
    assert!(invalid_duration_long.validate().is_err());
}

#[test]
fn test_threat_intelligence_json_serialization() {
    let threat = ThreatIntelligence::new(
        "172.16.0.1".to_string(),
        "ddos".to_string(),
        9,
        1200,
        "test-node".to_string(),
    )
    .with_description("Massive DDoS attack detected".to_string());

    // Serialize to JSON
    let json = threat.to_json().expect("Failed to serialize");
    assert!(json.contains("172.16.0.1"));
    assert!(json.contains("ddos"));
    assert!(json.contains("Massive DDoS attack detected"));

    // Deserialize from JSON
    let deserialized = ThreatIntelligence::from_json(&json).expect("Failed to deserialize");
    assert_eq!(deserialized.ip, threat.ip);
    assert_eq!(deserialized.threat_type, threat.threat_type);
    assert_eq!(deserialized.severity, threat.severity);
    assert_eq!(deserialized.block_duration_secs, threat.block_duration_secs);
    assert_eq!(deserialized.description, threat.description);
}

#[test]
fn test_threat_types() {
    let threat_types = vec![
        "syn_flood",
        "ddos",
        "brute_force",
        "port_scan",
        "malware",
        "botnet",
    ];

    for threat_type in threat_types {
        let threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            threat_type.to_string(),
            5,
            300,
            "test".to_string(),
        );
        assert_eq!(threat.threat_type, threat_type);
        assert!(threat.validate().is_ok());
    }
}

#[test]
fn test_severity_levels() {
    for severity in 1..=10 {
        let threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "test".to_string(),
            severity,
            300,
            "test".to_string(),
        );
        assert_eq!(threat.severity, severity);
        assert!(threat.validate().is_ok());
    }
}

#[test]
fn test_block_duration_ranges() {
    let valid_durations = vec![1, 60, 300, 600, 1800, 3600, 7200, 86400];

    for duration in valid_durations {
        let threat = ThreatIntelligence::new(
            "10.0.0.1".to_string(),
            "test".to_string(),
            5,
            duration,
            "test".to_string(),
        );
        assert_eq!(threat.block_duration_secs, duration);
        assert!(threat.validate().is_ok());
    }
}

#[test]
fn test_threat_with_description() {
    let threat = ThreatIntelligence::new(
        "192.168.1.50".to_string(),
        "brute_force".to_string(),
        6,
        900,
        "security-node".to_string(),
    );

    assert!(threat.description.is_none());

    let threat_with_desc = threat.with_description(
        "Multiple failed SSH login attempts detected from this IP".to_string(),
    );

    assert!(threat_with_desc.description.is_some());
    assert!(threat_with_desc
        .description
        .unwrap()
        .contains("SSH login"));
}

#[test]
fn test_threat_timestamp() {
    let threat = ThreatIntelligence::new(
        "10.0.0.1".to_string(),
        "test".to_string(),
        5,
        300,
        "test".to_string(),
    );

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    // Timestamp should be close to current time (within 5 seconds)
    assert!(threat.timestamp >= now - 5);
    assert!(threat.timestamp <= now + 5);
}

#[tokio::test]
async fn test_p2p_sender_channel() {
    let config = P2PConfig::default();
    let p2p = ThreatIntelP2P::new(config).expect("Failed to create P2P");

    let sender = p2p.get_sender();

    let threat = ThreatIntelligence::new(
        "10.0.0.1".to_string(),
        "test".to_string(),
        5,
        300,
        "test".to_string(),
    );

    // Send through channel
    let result = sender.send(threat);
    assert!(result.is_ok(), "Failed to send through channel");
}

#[test]
fn test_multiple_threats_batch() {
    let mut threats = Vec::new();

    for i in 1..=10 {
        let threat = ThreatIntelligence::new(
            format!("10.0.0.{}", i),
            "batch_test".to_string(),
            (i % 10) + 1,
            300,
            format!("node-{}", i),
        );
        threats.push(threat);
    }

    assert_eq!(threats.len(), 10);

    // All should be valid
    for threat in &threats {
        assert!(threat.validate().is_ok());
    }

    // Serialize all
    for threat in &threats {
        let json = threat.to_json();
        assert!(json.is_ok());
    }
}

#[test]
fn test_p2p_config_customization() {
    let mut config = P2PConfig::default();
    assert_eq!(config.listen_port, 9001);
    assert!(config.enable_mdns);
    assert!(config.bootstrap_peers.is_empty());

    config.listen_port = 8080;
    config.enable_mdns = false;

    assert_eq!(config.listen_port, 8080);
    assert!(!config.enable_mdns);
}

#[test]
fn test_edge_case_ips() {
    let edge_ips = vec![
        ("0.0.0.0", true),       // Minimum IP
        ("255.255.255.255", true), // Maximum IP
        ("127.0.0.1", true),     // Localhost
        ("192.168.1.1", true),   // Private
        ("10.0.0.1", true),      // Private
        ("172.16.0.1", true),    // Private
        ("8.8.8.8", true),       // Public
        ("256.0.0.1", false),    // Invalid
        ("1.1.1", false),        // Invalid
        ("not-an-ip", false),    // Invalid
    ];

    for (ip, should_be_valid) in edge_ips {
        let threat = ThreatIntelligence::new(
            ip.to_string(),
            "test".to_string(),
            5,
            300,
            "test".to_string(),
        );

        let is_valid = threat.validate().is_ok();
        assert_eq!(
            is_valid, should_be_valid,
            "IP {} validation mismatch",
            ip
        );
    }
}
