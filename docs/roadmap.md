# Development roadmap for the email intelligence pipeline

## Phase 1: Establish deterministic ingestion

### Step: Build normaliser service

- [ ] Implement `limela-normalise` gRPC streaming from `BlobRef` to
      `ParsedEmail`, enforcing deterministic `EmailId` hashing; acceptance:
      fixtures must cover multipart, malformed, and non-ASCII headers.
- [ ] Integrate MIME traversal using the selected crate with benchmarks proving
      ≤50 ms parse time for the 95th percentile 1 MB emails; acceptance:
      criterion benchmarks published in the repository.
- [ ] Add header decoding pipeline with RFC 2047 coverage and unit tests for
      encoded names, subjects, and addresses; acceptance: ≥90% path coverage
      for the decoder module.
- [ ] Wire the object storage client for inline versus blob body handling and
      document fallback rules in `limela-config`; acceptance: configuration
      documentation updated and an end-to-end fixture stores HTML bodies over
      2 MB as blobs.

### Step: Harden purification flow

- [ ] Deliver `limela-purify` streaming gRPC service emitting `PurifiedDoc`,
      including reply-chain stripping and signature removal heuristics;
      acceptance: golden tests for Gmail, Outlook, and plain RFC samples.
- [ ] Create configurable ruleset loader (YAML) with hot-reload support and a
      validation CLI; acceptance: schema validation errors surface with
      actionable messages.
- [ ] Add back-pressure aware ingestion to the coordinator DAG ensuring bounded
      queues with metrics; acceptance: load test demonstrates zero dropped
      messages at 10k messages per minute.

## Phase 2: Embed and persist semantic features

### Step: Stand up the dual-model embedding stage

- [ ] Implement `limela-embed` service generating SBERT and ColBERT outputs per
      `MetaEmbedding`; acceptance: integration test confirms vector
      dimensionality and token counts.
- [ ] Provision the embedding store interface for ColBERT shard persistence
      with resumable uploads; acceptance: failure injection test proves
      idempotent replays.
- [ ] Optimise batch scheduling with Rayon parallelism and configurable
      concurrency; acceptance: profiling demonstrates ≥70% CPU utilisation on
      an 8-core node without queue starvation.
- [ ] Add observability spans and metrics for token throughput, latency
      buckets, and error rates exposed via `limela-obsv`; acceptance: Grafana
      dashboard panels documented.

### Step: Define feature governance

- [ ] Establish regression suite comparing embeddings against a reference corpus
      to guard model drift; acceptance: CI job fails on cosine similarity delta
      greater than 0.05.
- [ ] Produce model card and risk assessment for selected SBERT and ColBERT
      checkpoints stored in documentation; acceptance: provenance and licensing
      recorded in the docs.

## Phase 3: Integrate clustering workflow

### Step: Integrate FISHDBC clustering

- [ ] Implement `limela-cluster` service consuming `MetaEmbedding` and emitting
      `ClusterAssignment`, honouring probability metadata; acceptance:
      deterministic clustering on a seeded dataset with a replayable snapshot.
- [ ] Build candidate search abstraction combining SBERT approximate nearest
      neighbour lookup and ColBERT re-ranking; acceptance: latency under
      150 ms p95 on a corpus of 100,000 vectors in the benchmark harness.
- [ ] Persist cluster lineage metadata for reprocessing audits in
      `limela-types`; acceptance: audit log entries accessible via the API
      gateway stub.
- [ ] Add concept drift detection hook signalling when probability entropy
      exceeds a defined threshold; acceptance: alert fires in a synthetic drift
      scenario.

### Step: Coordinate DAG orchestration

- [ ] Extend the `limela-coordinator` DAG specification to include conditional
      fan-out to clustering and knowledge graph ingestion with retry policies;
      acceptance: chaos test shows successful recovery from a single-stage
      crash.
- [ ] Document operational runbooks outlining deployment order, rollback, and
      health probes; acceptance: Markdown passes linting and aligns with ADR
      constraints.

## Phase 4: Record knowledge graph facts

### Step: Implement `limela-kg` ingestion path

- [ ] Deliver streaming `KgIngester::Ingest` handler mapping cluster outputs to
      Oxigraph triples per ADR-001 scope; acceptance: conformance tests verify
      predicates and tenant isolation.
- [ ] Create delta publisher to Telephone with at-least-once guarantees and
      idempotent duplication handling; acceptance: soak test confirms no
      missing deltas over a 24-hour replay.
- [ ] Automate per-tenant backup using `Store::backup()` with object storage
      rotation; acceptance: restore drill demonstrates zero data loss during a
      simulated failure.
- [ ] Expose an admin API for snapshot status, queue depth, and last applied
      email; acceptance: metrics scraped by the observability stack.

### Step: Secure and validate knowledge graph data

- [ ] Enforce schema validation ensuring only headers, structural metadata,
      cluster IDs, and encrypted embedding references are inserted; acceptance:
      rejection tests block disallowed body content.
- [ ] Implement tenancy authentication filters aligned with API gateway
      policies and network segmentation; acceptance: penetration test proves
      cross-tenant queries are blocked.
- [ ] Build SPARQL acceptance suite covering representative query patterns for
      Telephone; acceptance: results match golden outputs for sample tenants.

## Phase 5: Deliver end-to-end assurance and rollout

### Step: Provide integration coverage

- [ ] Assemble full pipeline smoke test harness replaying an anonymised mailbox
      fixture through all stages; acceptance: CI workflow completes within
      20 minutes and publishes trace artefacts.
- [ ] Implement synthetic load generator to stress ingestion, embedding,
      clustering, and knowledge graph recording concurrently; acceptance:
      report documents saturation points and scaling guidance.
- [ ] Add continuous verification jobs comparing cluster assignments between
      versions to flag regressions; acceptance: automated diffs stored for
      triage.

### Step: Achieve operational readiness

- [ ] Finalise SLOs for latency, throughput, and data freshness with dashboards
      and alerting thresholds; acceptance: SRE sign-off recorded in
      documentation.
- [ ] Prepare rollout checklist covering phased tenant enablement, rollback
      triggers, and the communication plan; acceptance: checklist reviewed by
      the architecture group.

Out of scope for this roadmap: IMAP write-back automation, dashboard
visualisation, and downstream Telephone rule authoring. These follow once the
core pipeline reaches production readiness.
