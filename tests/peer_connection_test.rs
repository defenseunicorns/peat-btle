// Copyright (c) 2025-2026 (r)evolve - Revolve Team LLC
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test peer connection state tracking and emergency/ACK flow
//!
//! Verifies that eche-btle properly tracks peer connection state
//! and handles emergency/ACK propagation.

use eche_btle::eche_mesh::{EcheMesh, EcheMeshConfig};
use eche_btle::observer::DisconnectReason;
use eche_btle::NodeId;

// Valid timestamp for testing (2024-01-15 00:00:00 UTC)
const TEST_TIMESTAMP: u64 = 1705276800000;

/// Test that receiving data from a peer marks them as connected
#[test]
fn test_peer_marked_connected_on_data_receive() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Initially, mesh_a has no peers
    assert_eq!(
        mesh_a.get_peers().len(),
        0,
        "Mesh A should start with no peers"
    );

    // Build document from mesh_b
    let doc_b = mesh_b.build_document();
    println!("Doc from B: {} bytes", doc_b.len());

    // Mesh A receives data from "peer-b" identifier
    let result = mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);
    assert!(
        result.is_some(),
        "on_ble_data should return Some for valid peer data"
    );

    // Now mesh_a should have peer_b registered
    let peers = mesh_a.get_peers();
    assert_eq!(
        peers.len(),
        1,
        "Mesh A should have 1 peer after receiving data"
    );

    // The peer should be marked as connected
    let peer_b = &peers[0];
    assert_eq!(peer_b.node_id, node_b);
    assert!(
        peer_b.is_connected,
        "Peer B should be marked as CONNECTED after receiving data"
    );

    println!(
        "PASS: Peer {} is_connected={}",
        peer_b.node_id.as_u32(),
        peer_b.is_connected
    );
}

/// Test full emergency/ACK flow between two nodes
#[test]
fn test_emergency_ack_flow() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Step 1: Exchange initial documents to register peers
    let doc_a = mesh_a.build_document();
    let doc_b = mesh_b.build_document();

    let result_a = mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);
    let result_b = mesh_b.on_ble_data("peer-a", &doc_a, TEST_TIMESTAMP);

    assert!(result_a.is_some(), "A should process B's document");
    assert!(result_b.is_some(), "B should process A's document");

    // Verify peers are registered
    assert_eq!(mesh_a.get_peers().len(), 1, "A should have 1 peer");
    assert_eq!(mesh_b.get_peers().len(), 1, "B should have 1 peer");

    println!("Step 1 PASS: Both nodes see each other as peers");

    // Step 2: Node A sends EMERGENCY
    let emergency_doc = mesh_a.start_emergency_with_known_peers(TEST_TIMESTAMP + 1000);
    println!("Emergency doc from A: {} bytes", emergency_doc.len());

    // Verify A has active emergency
    assert!(
        mesh_a.has_active_emergency(),
        "A should have active emergency after start_emergency"
    );

    let status_a = mesh_a.get_emergency_status();
    assert!(status_a.is_some(), "A should have emergency status");
    let (source, ts, acked, pending) = status_a.unwrap();
    println!(
        "A's emergency: source={:08X} ts={} acked={} pending={}",
        source, ts, acked, pending
    );
    assert_eq!(source, node_a.as_u32(), "Emergency source should be A");

    println!("Step 2 PASS: A created emergency");

    // Step 3: Node B receives emergency document
    let result_b = mesh_b.on_ble_data("peer-a", &emergency_doc, TEST_TIMESTAMP + 1100);
    assert!(
        result_b.is_some(),
        "B should process A's emergency document"
    );

    let result = result_b.unwrap();
    println!(
        "B received: emergency={} ack={} counter_changed={} emergency_changed={}",
        result.is_emergency, result.is_ack, result.counter_changed, result.emergency_changed
    );

    // B should now have the emergency
    assert!(
        mesh_b.has_active_emergency(),
        "B should have active emergency after receiving"
    );

    let status_b = mesh_b.get_emergency_status();
    assert!(status_b.is_some(), "B should have emergency status");
    let (source_b, ts_b, acked_b, pending_b) = status_b.unwrap();
    println!(
        "B's emergency: source={:08X} ts={} acked={} pending={}",
        source_b, ts_b, acked_b, pending_b
    );
    assert_eq!(
        source_b,
        node_a.as_u32(),
        "B's emergency source should be A"
    );

    println!("Step 3 PASS: B received emergency");

    // Step 4: Node B sends ACK
    let ack_doc = mesh_b.ack_emergency(TEST_TIMESTAMP + 1200);
    assert!(ack_doc.is_some(), "B should be able to ACK the emergency");
    let ack_doc = ack_doc.unwrap();
    println!("ACK doc from B: {} bytes", ack_doc.len());

    // Verify B has acked (in its own state)
    assert!(
        mesh_b.has_peer_acked(node_b.as_u32()),
        "B should show itself as acked"
    );

    println!("Step 4 PASS: B created ACK");

    // Step 5: Node A receives ACK
    let result_a = mesh_a.on_ble_data("peer-b", &ack_doc, TEST_TIMESTAMP + 1300);
    assert!(result_a.is_some(), "A should process B's ACK document");

    let result = result_a.unwrap();
    println!(
        "A received: emergency={} ack={} counter_changed={} emergency_changed={}",
        result.is_emergency, result.is_ack, result.counter_changed, result.emergency_changed
    );

    // A should now see B as acked
    let a_sees_b_acked = mesh_a.has_peer_acked(node_b.as_u32());
    println!("A sees B acked: {}", a_sees_b_acked);
    assert!(a_sees_b_acked, "A should see B as having ACKed");

    println!("Step 5 PASS: A received B's ACK");

    // Final verification
    let status_a = mesh_a.get_emergency_status().unwrap();
    println!(
        "Final A status: acked={} pending={}",
        status_a.2, status_a.3
    );

    println!("\n=== FULL EMERGENCY/ACK FLOW TEST PASSED ===");
}

/// Test that multiple peers are properly tracked
#[test]
fn test_multiple_peer_registration() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);
    let node_c = NodeId::new(0xCCCCCCCC);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");
    let config_c = EcheMeshConfig::new(node_c, "CHARLIE", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);
    let mesh_c = EcheMesh::new(config_c);

    // Build documents
    let _doc_a = mesh_a.build_document();
    let doc_b = mesh_b.build_document();
    let doc_c = mesh_c.build_document();

    // A receives from B and C
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);
    mesh_a.on_ble_data("peer-c", &doc_c, TEST_TIMESTAMP + 100);

    // A should have 2 peers, both connected
    let peers_a = mesh_a.get_peers();
    assert_eq!(peers_a.len(), 2, "A should have 2 peers");

    for peer in &peers_a {
        println!(
            "Peer {:08X}: is_connected={}",
            peer.node_id.as_u32(),
            peer.is_connected
        );
        assert!(peer.is_connected, "All peers should be connected");
    }

    // Verify connected peer count
    let connected = mesh_a.get_connected_peers();
    assert_eq!(connected.len(), 2, "A should have 2 connected peers");

    println!("PASS: Multiple peers registered correctly");
}

/// Test that peers are marked disconnected when BLE disconnect occurs
#[test]
fn test_peer_marked_disconnected_on_ble_disconnect() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Build and exchange documents
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);

    // Verify peer B is connected
    let peers = mesh_a.get_peers();
    assert_eq!(peers.len(), 1, "A should have 1 peer");
    assert!(
        peers[0].is_connected,
        "Peer B should be connected initially"
    );
    println!(
        "Before disconnect: peer is_connected={}",
        peers[0].is_connected
    );

    // Simulate BLE disconnect (like turning BLE off)
    let disconnected_node = mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LinkLoss);
    assert!(
        disconnected_node.is_some(),
        "on_ble_disconnected should return the disconnected node"
    );
    assert_eq!(
        disconnected_node.unwrap(),
        node_b,
        "Disconnected node should be B"
    );

    // Verify peer B is now marked as disconnected
    let peers_after = mesh_a.get_peers();
    assert_eq!(
        peers_after.len(),
        1,
        "Peer should still exist but be disconnected"
    );
    assert!(
        !peers_after[0].is_connected,
        "Peer B should be DISCONNECTED after on_ble_disconnected"
    );

    println!(
        "After disconnect: peer is_connected={}",
        peers_after[0].is_connected
    );
    println!("PASS: Peer marked disconnected on BLE disconnect");
}

/// Test that disconnected peers are removed after timeout via tick()
#[test]
fn test_stale_disconnected_peer_cleanup() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    // Use short timeout for testing (5 seconds)
    // Note: cleanup_interval is 10s by default, so we need to use timestamps
    // that account for both the peer_timeout AND cleanup_interval
    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST").with_peer_timeout(5000);
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Build and exchange documents at t=1000 (use small timestamps for tick() internal tracking)
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, 1000);

    // Verify peer is connected
    assert_eq!(mesh_a.get_peers().len(), 1, "A should have 1 peer");
    assert!(
        mesh_a.get_peers()[0].is_connected,
        "Peer should be connected"
    );

    // Disconnect the peer
    mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LinkLoss);
    assert!(
        !mesh_a.get_peers()[0].is_connected,
        "Peer should be disconnected"
    );

    // First tick initializes last_cleanup_ms
    mesh_a.tick(2000);
    assert_eq!(
        mesh_a.get_peers().len(),
        1,
        "Peer should still exist (not stale yet)"
    );

    // Tick at t=20000 - peer is stale (19 seconds since last data at t=1000, 5 second timeout)
    // and cleanup_interval (10s) has elapsed since last tick at t=2000
    mesh_a.tick(20000);
    let peers_after_cleanup = mesh_a.get_peers();
    assert_eq!(
        peers_after_cleanup.len(),
        0,
        "Stale disconnected peer should be removed after timeout"
    );

    println!("PASS: Stale disconnected peer cleaned up after timeout");
}

/// Test that connected peers become stale if no data is received
/// (simulates BLE off without disconnect event)
#[test]
fn test_connected_peer_becomes_stale() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    // Use short timeout for testing (5 seconds)
    // Note: cleanup runs every 10s by default
    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST").with_peer_timeout(5000);
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Build and exchange documents at t=1000 (use small timestamps)
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, 1000);

    // Verify peer is connected
    let peers = mesh_a.get_peers();
    assert_eq!(peers.len(), 1, "A should have 1 peer");
    assert!(peers[0].is_connected, "Peer should be connected initially");
    println!(
        "Initial state: peer is_connected={}, last_seen=1000",
        peers[0].is_connected
    );

    // First tick at t=3000 - initializes internal tracking, peer still fresh
    mesh_a.tick(3000);
    assert_eq!(
        mesh_a.get_peers().len(),
        1,
        "Peer should still exist after 3s"
    );

    // Tick at t=20000 - peer is now stale (19s since t=1000, > 5s timeout)
    // and cleanup_interval (10s) has elapsed since tick at t=3000
    mesh_a.tick(20000);
    let peers_after = mesh_a.get_peers();
    assert_eq!(
        peers_after.len(),
        0,
        "Stale 'connected' peer should be removed when no data received for timeout period"
    );

    println!("PASS: Connected peer becomes stale and is removed");
}

/// Test multiple disconnect/reconnect cycles
#[test]
fn test_reconnect_after_disconnect() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Initial connection
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);
    assert!(
        mesh_a.get_peers()[0].is_connected,
        "Peer should be connected after initial data"
    );

    // Disconnect
    mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LocalRequest);
    assert!(
        !mesh_a.get_peers()[0].is_connected,
        "Peer should be disconnected"
    );
    println!("After disconnect: is_connected=false");

    // Reconnect with new data
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP + 1000);
    let peers = mesh_a.get_peers();
    assert_eq!(
        peers.len(),
        1,
        "Should still have 1 peer (updated, not duplicated)"
    );
    assert!(
        peers[0].is_connected,
        "Peer should be connected again after receiving new data"
    );
    println!("After reconnect: is_connected=true");

    // Disconnect again (simulating toggling BLE)
    mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LinkLoss);
    assert!(
        !mesh_a.get_peers()[0].is_connected,
        "Peer should be disconnected again"
    );

    println!("PASS: Reconnect after disconnect works correctly");
}

/// Test that different disconnect reasons are handled properly
#[test]
fn test_various_disconnect_reasons() {
    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    let doc_b = mesh_b.build_document();

    // Test each disconnect reason
    let reasons = [
        (DisconnectReason::LocalRequest, "LocalRequest"),
        (DisconnectReason::RemoteRequest, "RemoteRequest"),
        (DisconnectReason::Timeout, "Timeout"),
        (DisconnectReason::LinkLoss, "LinkLoss"),
        (DisconnectReason::ConnectionFailed, "ConnectionFailed"),
        (DisconnectReason::Unknown, "Unknown"),
    ];

    for (reason, reason_name) in reasons {
        // Connect
        mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);
        assert!(
            mesh_a.get_peers()[0].is_connected,
            "Peer should be connected before {} disconnect",
            reason_name
        );

        // Disconnect with specific reason
        let result = mesh_a.on_ble_disconnected("peer-b", reason);
        assert!(
            result.is_some(),
            "{} disconnect should return Some",
            reason_name
        );
        assert!(
            !mesh_a.get_peers()[0].is_connected,
            "Peer should be disconnected after {}",
            reason_name
        );

        println!("{}: disconnect handled correctly", reason_name);
    }

    println!("PASS: All disconnect reasons handled correctly");
}

/// Test that connection graph is updated when peers disconnect
#[test]
fn test_connection_graph_updated_on_disconnect() {
    use eche_btle::peer::ConnectionState;

    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);

    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST");
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST");

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Build and exchange documents
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);

    // Verify peer B is connected
    let peers = mesh_a.get_peers();
    assert_eq!(peers.len(), 1, "A should have 1 peer");
    assert!(peers[0].is_connected, "Peer B should be connected");

    // Get connection graph state before disconnect
    let graph_before = mesh_a.get_connection_graph();
    let connected_before: Vec<_> = graph_before
        .iter()
        .filter(|p| p.state.is_connected())
        .collect();
    assert_eq!(
        connected_before.len(),
        1,
        "Graph should show 1 connected peer before disconnect"
    );
    println!(
        "Before disconnect: {} connected in graph",
        connected_before.len()
    );

    // Disconnect
    mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LinkLoss);

    // Verify peer B is disconnected in PeerManager
    let peers_after = mesh_a.get_peers();
    assert!(
        !peers_after[0].is_connected,
        "Peer B should be disconnected"
    );

    // Verify connection graph is updated
    let graph_after = mesh_a.get_connection_graph();
    let connected_after: Vec<_> = graph_after
        .iter()
        .filter(|p| p.state.is_connected())
        .collect();
    assert_eq!(
        connected_after.len(),
        0,
        "Graph should show 0 connected peers after disconnect"
    );

    // The peer should be in Disconnected state in the graph
    let peer_state = mesh_a.get_peer_connection_state(node_b);
    assert!(
        peer_state.is_some(),
        "Peer should still exist in graph (in disconnected state)"
    );
    assert_eq!(
        peer_state.as_ref().unwrap().state,
        ConnectionState::Disconnected,
        "Peer should be in Disconnected state"
    );

    println!(
        "After disconnect: {} connected in graph, peer state={:?}",
        connected_after.len(),
        peer_state.unwrap().state
    );
    println!("PASS: Connection graph updated on disconnect");
}

/// Test that indirect peers are cleaned up when their via_peer disconnects
#[test]
fn test_indirect_peers_cleaned_on_via_peer_disconnect() {
    use eche_btle::relay::RelayEnvelope;

    let node_a = NodeId::new(0xAAAAAAAA);
    let node_b = NodeId::new(0xBBBBBBBB);
    let node_c = NodeId::new(0xCCCCCCCC);

    // Enable relay to track indirect peers
    let config_a = EcheMeshConfig::new(node_a, "ALPHA", "TEST").with_relay();
    let config_b = EcheMeshConfig::new(node_b, "BRAVO", "TEST").with_relay();

    let mesh_a = EcheMesh::new(config_a);
    let mesh_b = EcheMesh::new(config_b);

    // Build and exchange documents to establish direct connection A <-> B
    let doc_b = mesh_b.build_document();
    mesh_a.on_ble_data("peer-b", &doc_b, TEST_TIMESTAMP);

    // Verify B is connected
    assert_eq!(mesh_a.get_connected_peers().len(), 1);

    // Create a relay envelope that looks like it came from C via B
    // C originates the message, B relays it (so hop_count is 1 when A receives it)
    let config_c = EcheMeshConfig::new(node_c, "CHARLIE", "TEST").with_relay();
    let mesh_c = EcheMesh::new(config_c);
    let c_doc = mesh_c.build_document();

    // Create envelope as if C sent it, then B relayed it (increment hop)
    let envelope = RelayEnvelope::broadcast(node_c, c_doc).with_max_hops(7);
    let relayed = envelope.relay().expect("Should be able to relay"); // This increments hop_count to 1
    let relay_data = relayed.encode();

    // Process the relay document as if it came from B (but originated from C)
    mesh_a.on_ble_data("peer-b", &relay_data, TEST_TIMESTAMP + 1000);

    // Verify C is tracked as indirect peer
    let indirect_before = mesh_a.get_indirect_peers();
    let c_indirect = indirect_before.iter().find(|p| p.node_id == node_c);
    assert!(c_indirect.is_some(), "C should be tracked as indirect peer");
    assert_eq!(
        c_indirect.unwrap().min_hops,
        1,
        "C should be 1 hop away via B"
    );
    println!(
        "Before disconnect: C is indirect peer via B ({} hops), total indirect: {}",
        c_indirect.unwrap().min_hops,
        indirect_before.len()
    );

    // Disconnect B
    let disconnected = mesh_a.on_ble_disconnected("peer-b", DisconnectReason::LinkLoss);
    println!(
        "Disconnected node: {:?}",
        disconnected.map(|n| format!("{:08X}", n.as_u32()))
    );
    assert_eq!(
        disconnected,
        Some(node_b),
        "Should have disconnected node B"
    );

    // Verify C is no longer tracked (path through B is gone)
    let indirect_after = mesh_a.get_indirect_peers();
    println!(
        "Indirect peers after disconnect: {:?}",
        indirect_after
            .iter()
            .map(|p| format!("{:08X} (via {:?})", p.node_id.as_u32(), p.via_peers))
            .collect::<Vec<_>>()
    );
    let c_after = indirect_after.iter().find(|p| p.node_id == node_c);
    assert!(
        c_after.is_none(),
        "C should be removed when via_peer B disconnects"
    );
    println!(
        "After disconnect: indirect peers remaining: {}",
        indirect_after.len()
    );

    println!("PASS: Indirect peers cleaned up when via_peer disconnects");
}
