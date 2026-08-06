//! Local collaboration load-test harness (best-effort, no external infra).
//!
//! SPDX-License-Identifier: MIT OR Apache-2.0
//!
//! Simulates `N` concurrent replicas editing a shared room through an in-process
//! [`SyncHub`] over loopback, then asserts every replica converges to the same
//! document. This is a local proxy for the large-scale infra load test (which is
//! still deferred): it validates merge correctness and convergence latency under
//! concurrent edits, not network throughput or multi-node scaling.

use std::time::Instant;

use tpt_vertex_collab::{
    AccessLevel, ClientMessage, CrdtDoc, LocalReplica, MemoryAuth, ParamValue, ReplicaId,
    ServerMessage, SyncHub,
};

const REPLICAS: usize = 8;
const STEPS: usize = 60;

fn build_hub() -> SyncHub<MemoryAuth> {
    let mut auth = MemoryAuth::new();
    auth.add_token("tok", "user");
    auth.set_default_level(Some(AccessLevel::Editor));
    let mut hub = SyncHub::new(auth);
    for i in 0..REPLICAS {
        hub.handle(
            (i as u64) + 1,
            ClientMessage::Join {
                room: "room".into(),
                token: "tok".into(),
                replica: ReplicaId(i as u64),
                display_name: format!("u{i}"),
            },
        );
    }
    hub
}

#[test]
fn concurrent_replicas_converge_under_load() {
    let mut hub = build_hub();

    // Per-replica local state + the feature keys each has created (so we can
    // drive parameter edits at them).
    let mut replicas: Vec<LocalReplica> = (0..REPLICAS)
        .map(|i| LocalReplica::new(ReplicaId(i as u64)))
        .collect();
    let mut keys: Vec<Vec<u64>> = vec![Vec::new(); REPLICAS];

    let start = Instant::now();
    for step in 0..STEPS {
        for i in 0..REPLICAS {
            // Alternate between adding a feature and editing one of this
            // replica's existing features' parameters.
            let op = if step % 3 == 0 || keys[i].is_empty() {
                let (k, add) = replicas[i].add_feature("Extrude", "a");
                keys[i].push(k);
                add
            } else {
                let k = keys[i][step % keys[i].len()];
                replicas[i].set_param(k, "height", ParamValue::Number(step as f64 + i as f64))
            };

            // Feed the op to the hub, then deliver any broadcast ops to the
            // other replicas (loopback delivery).
            let outbound = hub.handle((i as u64) + 1, ClientMessage::Ops { ops: vec![op] });
            for o in outbound {
                let idx = (o.to - 1) as usize;
                if let ServerMessage::Ops { ops, .. } = o.message {
                    for broadcast in &ops {
                        replicas[idx].receive(broadcast);
                    }
                }
            }
        }
    }
    let elapsed = start.elapsed();
    let total_ops = REPLICAS * STEPS;
    eprintln!(
        "collab load test: {total_ops} ops across {REPLICAS} replicas in {elapsed:?} \
         ({:.0} ops/s)",
        total_ops as f64 / elapsed.as_secs_f64()
    );

    // Every replica must converge to the hub's authoritative document.
    let authoritative: CrdtDoc = hub.document("room").expect("room exists").clone();
    assert!(!authoritative.is_empty(), "expected features to be created");
    for (i, r) in replicas.iter().enumerate() {
        assert_eq!(&r.doc, &authoritative, "replica {i} diverged from hub");
    }
}
