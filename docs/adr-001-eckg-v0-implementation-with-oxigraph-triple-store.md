# ADR-001: ECKG-v0 Implementation with Oxigraph Triple Store

**Status**: Accepted (Working Spec v0.1)

**Date**: 2025-10-29

**Decision Owner**: Limela architecture group

**Related**: *N/A*

## Context and problem statement

The Event-Centric Knowledge Graph (ECKG) turns processed email signals into an
event-style network of people, threads, and topics. For v0 the team requires:

- **Minimal but rich data**: Persist headers, structural metadata, cluster IDs,
  and encrypted pointers to embeddings instead of raw bodies so the graph stays
  lean and privacy-aware.

- **Event-centric schema**: Model each email as an event node connected to
  person nodes (senders/recipients), conversation threads, and topic clusters.
  Content-derived entities such as `MENTIONS` remain out of scope until a later
  iteration.

- **Dynamic updates and multi-tenancy**: Accept continuous inserts from the
  pipeline whilst keeping tenants isolated; support at-least-once delivery so a
  downstream reasoning engine can maintain an up-to-date view.

- **Integration with the Telephone inference engine**: Telephone consumes graph
  deltas to power GPU-accelerated differential Datalog. The storage layer must
  therefore stream facts without adding complex provenance logic.

## Decision

Implement `limela-kg` as a Rust microservice that embeds the Oxigraph RDF store
per tenant. Each instance:

- Runs as a Kubernetes StatefulSet with a persistent volume so data survives pod
  restarts and upgrades.

- Exposes an internal-only SPARQL 1.1 endpoint plus a small admin API for
  maintenance tasks such as snapshots.

- Writes triples generated from `KgTriple` messages and publishes the same
  deltas to Telephone over a durable stream.

- Stores only structural fields: email identifiers, header metadata, cluster
  membership, thread linkage, and an encrypted reference to ColBERT shards in
  the embedding store.

- Provides automated backups by invoking `Store::backup()` and copying the
  incrementally updated snapshot to cloud object storage.

## Rationale

- **Rust-native integration**: Keeping the service in Rust avoids cross-runtime
  overhead, allows direct linking with Oxigraph, and aligns with the rest of
  the pipeline.

- **Operational simplicity**: A single-node Oxigraph instance per tenant avoids
  distributed consensus, enables targeted restores, and maps cleanly onto the
  tenancy model of the wider platform.

- **Performance fit**: Oxigraph on RocksDB offers ACID transactions and solid
  OLTP characteristics for hundreds of millions of triples—well above the v0
  footprint.

- **Privacy by design**: Excluding bodies and embedding only encrypted
  references reduces blast radius if the store is compromised while retaining
  linkage to semantic services.

- **Clear separation of concerns**: Telephone performs GPU-accelerated
  inference and provenance reasoning, letting the storage tier focus on factual
  persistence and simple queries.

## Alternatives considered

- **Managed or polyglot graph databases (Neo4j, Neptune, JanusGraph)** were
  rejected because they introduce new stacks, licensing constraints, or network
  hops for every insert/query.

- **SQL or NoSQL stores** would require bespoke graph layers (recursive CTEs or
  denormalised adjacency documents) and still lack native SPARQL/Datalog
  semantics.

- **Telephone as the sole store** mixes transient inference state with durable
  storage and complicates recovery.

- **Message broker fan-out** for delta streaming remains an option for later
  revisions but is unnecessary for the initial per-tenant deployment.

## Consequences

**Positive:**

- High-throughput ingestion with immediate availability for Telephone and other
  trusted services.
- Tight control over data exposure thanks to internal-only networking and the
  absence of raw bodies.
- Straightforward per-tenant backup and restoration workflows.

**Risks and mitigations:**

- Single-node Oxigraph instances create a per-tenant single point of failure;
  mitigated with Kubernetes restarts, persistent volumes, and frequent
  snapshots.

- Oxigraph's relative youth requires proactive testing and monitoring; the team
  pins to stable releases and contributes fixes upstream when needed.

- Lack of built-in access control shifts responsibility to the API gateway and
  network policy; tight egress rules and service authentication are mandatory.

- Potential scale limits for exceptionally large tenants; mitigation includes
  sharding by time or migrating to a distributed triple store if growth demands
  it.

## References

- Oxigraph project documentation and performance evaluations.
- Telephone (GPU DDlog) design notes for delta ingestion requirements.
- Limela pipeline architecture blueprint.
