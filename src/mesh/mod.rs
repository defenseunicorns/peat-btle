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

//! Mesh Topology Management
//!
//! This module provides mesh topology management for ECHE-BTLE, including:
//!
//! - **Topology tracking**: Parent/child/peer relationships
//! - **Connection management**: Connect, disconnect, failover
//! - **Message routing**: Upward aggregation, downward dissemination
//! - **RSSI-based selection**: Best parent/peer selection
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────┐
//! │                    MeshManager                           │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────────┐  │
//! │  │  Topology   │  │   Router    │  │  Parent Failover │  │
//! │  │   State     │  │             │  │                  │  │
//! │  └─────────────┘  └─────────────┘  └─────────────────┘  │
//! └─────────────────────────────────────────────────────────┘
//!                          │
//!                          ▼
//!            ┌──────────────────────────────┐
//!            │     TopologyEvents           │
//!            │  • ParentConnected           │
//!            │  • ChildConnected            │
//!            │  • TopologyChanged           │
//!            └──────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! use eche_btle::mesh::{MeshManager, TopologyConfig, TopologyEvent};
//! use eche_btle::{NodeId, HierarchyLevel};
//!
//! // Create mesh manager
//! let manager = MeshManager::new(
//!     NodeId::new(0x12345678),
//!     HierarchyLevel::Platform,
//!     TopologyConfig::default(),
//! );
//!
//! // Register for topology events
//! manager.on_topology_event(Box::new(|event| {
//!     match event {
//!         TopologyEvent::ParentConnected { node_id, .. } => {
//!             println!("Connected to parent: {}", node_id);
//!         }
//!         _ => {}
//!     }
//! }));
//!
//! // Start the manager
//! manager.start()?;
//!
//! // Process discovered beacons
//! manager.process_beacon(&beacon, rssi);
//!
//! // Select and connect to best parent
//! if let Some(candidate) = manager.select_best_parent() {
//!     manager.connect_parent(candidate.node_id, candidate.level, candidate.rssi)?;
//! }
//! ```
//!
//! ## Parent Failover
//!
//! When parent connection is lost:
//!
//! 1. `start_failover()` is called
//! 2. Manager enters `Failover` state
//! 3. `ParentFailoverStarted` event is emitted
//! 4. Application scans for new parent candidates
//! 5. `complete_failover()` connects to new parent (or gives up)
//! 6. `ParentFailoverCompleted` event is emitted
//!
//! ## Message Routing
//!
//! The `MeshRouter` provides routing decisions:
//!
//! - **Upward**: To parent (aggregation)
//! - **Downward**: To children (dissemination)
//! - **Broadcast**: To all connected peers
//! - **Targeted**: To a specific node
//!
//! ```ignore
//! use eche_btle::mesh::{MeshRouter, RouteDirection};
//!
//! let router = MeshRouter::new(node_id, my_level);
//! let topology = manager.topology();
//!
//! let decision = router.route(RouteDirection::Upward, &topology);
//! if decision.routed {
//!     for next_hop in decision.next_hops {
//!         send_to(&next_hop, &message);
//!     }
//! }
//! ```

#[cfg(feature = "std")]
mod manager;
mod routing;
mod topology;

#[cfg(feature = "std")]
pub use manager::{ManagerState, MeshManager, TopologyCallback};
pub use routing::{HopTracker, MeshRouter, RouteDecision, RouteDirection, RouteFailure};
pub use topology::{
    ConnectionState, DisconnectReason, MeshTopology, ParentCandidate, PeerInfo, PeerRole,
    TopologyConfig, TopologyEvent,
};
