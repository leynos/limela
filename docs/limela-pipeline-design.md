# Architectural Blueprint for a High-Performance Email Intelligence Pipeline in Rust

## System Architecture Overview

### Introduction and Design Philosophy

This document provides a comprehensive architectural blueprint for a
high-performance data processing pipeline designed to transform raw email
streams into structured, actionable intelligence. The primary objective is to
construct a robust, scalable, and efficient system entirely in Rust, leveraging
the language's guarantees of memory safety, concurrency, and near-native
performance. The processed intelligence will feed two distinct downstream
systems: a dynamic, density-based clustering engine and an event-centric
knowledge graph.

The design is founded on four core tenets that guide all subsequent
architectural and implementation decisions:

1. **Performance:** The system must be capable of processing a high volume of
    emails with minimal latency. This is achieved through the selection of
    high-performance Rust crates, a focus on zero-copy data handling where
    possible, and the extensive use of data-parallelism to leverage multi-core
    processor architectures.

2. **Modularity:** Each stage of the pipeline---Normalization, Purification,
    Feature Extraction, Clustering, and Ingestion---is designed as a distinct,
    composable unit with well-defined data interfaces. This separation of
    concerns allows for independent development, testing, scaling, and
    maintenance of each component. For instance, computationally intensive
    stages can be scaled on different hardware from I/O-bound stages.

3. **Fidelity:** The pipeline prioritizes the preservation of semantic meaning
    throughout the transformation process. From the careful decoding of email
    headers to the selection of advanced natural language processing (NLP)
    models, every step is designed to minimize information loss and extract the
    most accurate representation of the original content.

4. **Adaptability:** A production intelligence system cannot be static. The
    design incorporates mechanisms for long-term maintenance and adaptation to
    evolving data patterns. This includes configurable cleaning rules and an
    integrated concept drift detection system to ensure the pipeline's accuracy
    and relevance over time.

### End-to-End Data Flow as a Directed Acyclic Graph (DAG)

The pipeline is modeled as a Directed Acyclic Graph (DAG), where each stage is
a node and the data flow between stages constitutes the edges. This formalizes
the processing flow and allows for clear definitions of inter-service
communication contracts. The processing of individual emails or batches of
emails is parallelized to maximize throughput.

The high-level data flow is as follows:

1. **Ingestion & Normalization (Stage 1):** A raw email is ingested, typically
    as a reference to a blob in an object store (e.g., S3/MinIO). Its MIME
    structure is parsed to extract metadata and the primary content body. This
    stage produces a `ParsedEmail` message.

2. **Purification (Stage 2):** The `ParsedEmail` message is consumed. The
    content body is converted to clean, plain text, and artifacts like reply
    chains and signatures are removed. This stage emits a `PurifiedDoc` message.

3. **Meta-Embedding (Stage 3):** The `PurifiedDoc` is consumed. The text is
    fed into the dual-model engine to generate SBERT and ColBERT embeddings.
    The SBERT vector is included directly in the output message, while the
    large ColBERT token embeddings are written to a high-performance embedding
    store. The stage emits a `MetaEmbedding` message containing the SBERT
    vector and references (`BlobRef`) to the ColBERT data.

4. **Clustering & Ingestion (Stages 4 & 5):** The `MetaEmbedding` message is
    fanned out to two downstream consumers:

    - **FISHDBC Clustering:** The message is used to add the email to the
        FISHDBC system. The SBERT vector is used for initial candidate search,
        and the ColBERT reference is used for high-precision scoring. This
        stage produces a `ClusterAssignment` message.

    - **Knowledge Graph Ingestion:** The purified text, metadata, and the
        final `ClusterAssignment` are processed to extract entities and
        relationships, which are then emitted as a stream of `KgTriple`
        messages for ingestion into a graph database.

5. **IMAP Write-Back (Stage 6):** The `ClusterAssignment` message is consumed
    by an IMAP writer service, which connects to the source mail server and
    applies the cluster ID as a metadata tag to the original message for
    client-side visibility.

Throughout this process, the `rayon` crate is employed to parallelize the
workload across available CPU cores for CPU-bound tasks within each stage.[^1]
Communication between stages is handled via a robust, pressure-aware RPC
framework.

### Target Systems and Their Requirements

The pipeline is designed to serve two sophisticated downstream analytical
systems, each with unique requirements.

- **FISHDBC Clustering System:** The purpose of this system is to
    autonomously discover thematic groups, conversation threads, and emergent
    topics within the email corpus without prior knowledge of what those topics
    might be. Its key requirement from the pipeline is a highly nuanced and
    accurate measure of semantic similarity between any two emails. This
    necessitates a custom distance metric that goes beyond simple vector
    similarity, which the ColBERT component of the meta-embedding is designed
    to provide.[^3]

- **Event-Centric Knowledge Graph:** This system models the email data as a
    network of entities and their interactions. An "event" is a central
    concept, represented by a set of relationships connecting entities, such as
    "Person A *emailed* Person B *about* Project X *on* Date Y." The KG
    requires the pipeline to supply not just raw text but also extracted,
    structured information: identified entities, resolved conversation threads
    (from email headers), and thematic context (from cluster assignments).

## Project Organization and Monorepo Structure

To maintain sane build times, enforce clear boundaries, and simplify dependency
management, the entire system will be organized within a single Cargo workspace
(a "monorepo").

- `limela/` (Workspace Root)

  - `crates/`

    - `limela-types`: Contains core domain structs (`EmailId`, `BlobRef`,
            etc.) with feature gates for `serde` and `prost`/`capnp` support.

    - `limela-proto`: Houses the `.proto` files and the generated IPC
            stubs from `tonic-build` and `prost-build`.

    - `limela-link`: Defines the abstract `Link<T>` trait for
            inter-service communication and provides concrete transport
            implementations (e.g., `link-grpc`, `link-uds`, and an optional
            `link-capnp`).

    - `limela-ingest-imap`: An IMAP/JMAP connector service responsible
            for fetching new mail and writing back cluster tags. Uses the
            `async-imap` crate.

    - `limela-normalise`,`limela-purify`,`limela-embed`,`limela-cluster`,`limela-kg`:
            Individual binaries for each pipeline stage.

    - `limela-coordinator`: The thin scheduler that models the pipeline
            using `daggy` and executes the dataflow.

    - `limela-obsv`: Centralized setup for `tracing`, OpenTelemetry, and
            metrics exportation.

    - `limela-config`: Configuration loading and management.

    - `limela-api-gateway`: An `axum` or `actix-web` based API gateway
            that exposes a REST or gRPC-Web interface for the dashboard.

  - `services/`: Contains minimal `[[bin]]` targets that launch the
        respective service crates from `crates/`.

  - `dashboard/`: A separate frontend application (e.g., Vite + React) that
        communicates with the `limela-api-gateway`.

  - `deploy/`: Holds deployment artifacts like Docker Compose files or Helm
        charts.

## Distributed Architecture and Dataflow Management

To ensure scalability, resilience, and operational clarity, the pipeline is
designed with a clean separation between the control plane (orchestration) and
the data plane (payload movement).

### Control and Data Planes

- **Control Plane:** This layer is responsible for job orchestration, service
    discovery, health monitoring (heartbeats), and managing back-pressure. The
    recommended implementation is **gRPC**, using the `tonic` and `prost`
    crates. gRPC's use of HTTP/2 provides robust, standards-based flow control,
    and its ecosystem integrates seamlessly with modern observability tools for
    tracing and load balancing.

- **Data Plane:** This layer handles the physical movement of data payloads.
    A tiered strategy is employed to optimize for performance and cost:

    1. **Inline Payloads:** Small, fixed-size data like headers, purified
        text, and SBERT vectors are embedded directly within gRPC messages.

    2. **Blob References (`BlobRef`):** Large, unstructured data artifacts,
        such as raw EML files, attachments, and especially the voluminous
        ColBERT token embeddings, are stored in a dedicated object store (e.g.,
        S3, MinIO). Messages then carry lightweight references (`BlobRef`) to
        these objects instead of the objects themselves. This "don't ship
        elephants" approach prevents network saturation and keeps IPC messages
        lean. The `rust-s3` or `minio-rs` crates can be used for this purpose.

    3. **Local Fast-Path:** For pipeline stages co-located on the same host,
        **Unix Domain Sockets (UDS)** can be used to bypass the network stack
        entirely, offering a high-throughput, low-latency communication
        channel. The `tokio::net::UnixSocket` API provides the necessary
        primitives for this.[^5]

### Inter-Service Communication (IPC) and Message Contracts

Strict, schema-defined contracts are essential for maintaining modularity and
ensuring data integrity between services.

#### Default Transport: gRPC with `tonic` and `prost`

The default transport mechanism for all inter-stage communication is gRPC. The
combination of `tonic` for the gRPC implementation and `prost` for Protobuf
code generation provides a reliable, high-performance baseline that is easy to
operate and profile.[^7] All services will expose bidirectional streaming
endpoints to naturally handle back-pressure via HTTP/2 flow control.

#### Surgical High-Performance Transport: Cap'n Proto

For specific performance-critical "hot loops," **Cap'n Proto** offers a
compelling alternative. Its zero-copy serialization format avoids the overhead
of encoding and decoding data, making it exceptionally fast for in-memory and
IPC scenarios.[^9] Its capability-based RPC system is particularly valuable for
advanced patterns where one service needs to grant another service access to a
resource (a "sink").[^11]

Potential use cases for `capnp-rpc` in this pipeline include:

- **Dynamic Fan-Out:** The embedding service could hand the downstream
    clustering and KG services a capability representing a per-email result
    sink, allowing for dynamic rewiring of consumers.

- **ColBERT Scoring Loop:** The clusterer could pass the ColBERT scorer a
    capability for a stream of candidate IDs, allowing the scorer to push back
    partial results immediately, reducing round-trip latency.

The pragmatic approach is to default to `tonic`/`prost` for its robustness and
ecosystem support, and introduce `capnp-rpc` only for specific,
benchmark-proven bottlenecks.

#### Protobuf Message Definitions

The following `.proto` definitions establish the contracts for data flowing
between the pipeline stages.

```protocol-buffers
syntax = "proto3";
package limela.v1;

// Shared identifier, deterministically generated for idempotency.
message EmailId {
  bytes blake3_128 = 1; // BLAKE3 hash of Message-ID + Date + From + Subject
}

// Reference to a large object in a blob store.
message BlobRef {
  string bucket = 1;
  string key = 2;
  uint64 bytes = 3;
}

// Attachment metadata.
message Attachment {
  string filename = 1;
  string mime = 2;
  uint64 size = 3;
}

// Output of Stage 1 (Normalization) -> Input to Stage 2 (Purification)
message ParsedEmail {
  EmailId id = 1;
  string subject = 2;
  string from = 3;
  repeated string to = 4;
  string in_reply_to = 5;
  repeated string references = 6;
  string date_rfc3339 = 7;
  repeated Attachment attachments = 8;
  oneof body {
    string html = 9;
    string text = 10;
    BlobRef html_blob = 11;
    BlobRef text_blob = 12;
  }
}

// Output of Stage 2 (Purification) -> Input to Stage 3 (Embedding)
message PurifiedDoc {
  EmailId id = 1;
  string text = 2;
}

// Output of Stage 3 (Embedding) -> Input to Stages 4 & 5
message MetaEmbedding {
  EmailId id = 1;
  // SBERT vector is small enough to be sent inline.
  repeated float sbert = 2; // e.g., 768 dimensions
  // ColBERT token vectors are large and stored by reference.
  repeated BlobRef colbert_shards = 3;
  uint32 token_count = 4;
}

// Output of Stage 4 (Clustering) -> Input to Stage 5 (KG Ingestion) & Stage 6 (IMAP Write-Back)
message ClusterAssignment {
  EmailId id = 1;
  uint64 cluster_id = 2;
  float probability = 3;
}

// Final output for the Knowledge Graph
message KgTriple {
  string subject = 1;
  string predicate = 2;
  string object = 3;
  string object_datatype = 4;
}

```

#### gRPC Service Definitions

```protocol-buffers
import "google/protobuf/empty.proto";

service Normaliser {
  // Parses a stream of raw email blobs.
  rpc Parse(stream BlobRef) returns (stream ParsedEmail);
}

service Purifier {
  // Cleans a stream of parsed emails.
  rpc Clean(stream ParsedEmail) returns (stream PurifiedDoc);
}

service Embedder {
  // Generates embeddings for a stream of purified documents.
  rpc Embed(stream PurifiedDoc) returns (stream MetaEmbedding);
}

service Clusterer {
  // Adds embeddings to the clustering model.
  rpc Add(stream MetaEmbedding) returns (stream ClusterAssignment);
}

service KgIngester {
  // Ingests a stream of triples into the knowledge graph.
  rpc Ingest(stream KgTriple) returns (google.protobuf.Empty);
}

```

### System Guarantees: Idempotency and Determinism

To build a resilient system where retries are safe, every stage must be
idempotent. This is achieved by generating a deterministic `EmailId` for each
unique email. The recommended approach is to use the **BLAKE3** cryptographic
hash function, provided by the `blake3` crate, on a concatenation of stable
email headers (`Message-ID`, `Date`, `From`, `Subject`).[^13] Downstream
services will then use this `EmailId` as a primary key for `UPSERT` operations,
ensuring that reprocessing the same email does not create duplicate entries.

## Stage 1: Ingestion and High-Fidelity Email Normalization

### MIME Parsing and Structure Traversal

The initial stage of the pipeline confronts the complexity of the MIME
standard, the format used to encode modern emails. Emails are frequently
multipart documents containing alternative versions of content (e.g.,
`text/plain` and `text/html`), nested email messages, and various file
attachments. A robust parsing strategy is therefore fundamental to avoiding
data loss.

The pipeline must be capable of recursively traversing the MIME tree of each
email. The primary goal is to identify the most content-rich body part for
analysis. The standard logic will be to prefer the `text/html` part when
available, as it typically contains richer formatting and structural
information than its `text/plain` counterpart. If no HTML part exists, the
`text/plain` version will be used.

The Rust ecosystem provides several capable crates for this task. A thorough
evaluation points towards `mail-parser` and `mailparse` as leading candidates.
`mail-parser` is particularly noted for being fast and robust, making it a
strong initial choice. The final selection should be based on a detailed
assessment of API ergonomics, error handling on malformed emails, and community
maintenance status.

### Header Decoding and Metadata Extraction

Email headers contain a wealth of structured metadata that is critical for both
clustering and knowledge graph construction. However, headers containing
non-ASCII characters are often encoded according to the RFC 2047 standard.
Failure to correctly decode these headers results in corrupted or unusable
metadata. To address this, the pipeline will integrate a specialized crate such
as `rfc2047-decoder` to ensure the correct conversion of all header fields to
standard UTF-8 strings.

The following essential header fields must be extracted and preserved:

- **Identity and Routing:** `From`, `To`, `Cc`, `Bcc`

- **Content:** `Subject`, `Date`

- **Threading and Uniqueness:** `Message-ID`, `In-Reply-To`, `References`

The reliable extraction of the `Message-ID`, `In-Reply-To`, and `References`
headers is of paramount importance. These fields explicitly define the reply
structure of email conversations, forming a directed graph of communication.
This explicit graph structure is an invaluable input for the knowledge graph,
providing a foundational layer of relationships that semantic analysis can then
enrich. Ignoring this metadata would force the downstream systems to infer
relationships that are already unambiguously stated in the data.

### Attachment Handling Strategy

While the primary focus of this pipeline is the textual content of email
bodies, attachments are an important source of metadata. The pipeline will not
process the content of attachments initially, but it will identify and catalog
them. Using a crate like `mime_guess` (for extension-based guessing) or `infer`
(for magic number-based identification), the system will determine the MIME
type of each attachment. This information, along with the filename and size,
will be stored as metadata associated with the email (e.g.,
`attachment_count: 2`, `attachment_types: ["application/pdf", "image/jpeg"]`).
This allows the system to use the presence of certain attachment types as a
feature and flags attachments for potential processing by separate, specialized
pipelines in the future.

## Stage 2: Semantic Content Extraction and Purification

### HTML to Plain Text Conversion: A Fidelity vs. Performance Trade-off

After the most suitable MIME part is selected, its content must be converted
into a clean, plain text format suitable for NLP models. For HTML bodies, this
involves stripping away layout and styling tags while preserving the underlying
semantic structure of the text, such as paragraphs, lists, and headings.

This conversion presents a critical trade-off between rendering fidelity and
processing performance. The Rust ecosystem offers several options with
different characteristics. A detailed comparison of available crates reveals a
clear spectrum.[^15]

- **High-Fidelity Option:** The `html2text` crate uses `html5ever`, the
    browser-grade HTML parsing engine from Mozilla's Servo project. This
    ensures a highly accurate and robust conversion that correctly handles
    complex and even malformed HTML, preserving the semantic intent of the
    original message.

- **High-Performance Option:** The `nanohtml2text` crate is a zero-dependency
    alternative designed for speed. Benchmarks show it can be significantly
    faster and more memory-efficient than `html5ever`-based parsers.[^15]
    However, its simpler parsing logic may be less resilient to complex or
    non-standard HTML, potentially leading to lower-quality text output.

For this intelligence pipeline, where the quality of the input text directly
impacts the accuracy of the multi-million parameter embedding models,
**fidelity is paramount**. Therefore, the recommended approach is to use
`html2text`. The marginal performance cost is a worthwhile investment to ensure
the highest quality input for the subsequent, more computationally expensive
stages.

### Advanced Text Cleaning with the `regex` Crate

Raw text extracted from emails is rife with noise that can severely degrade the
performance of semantic models. This noise includes quoted reply chains, sender
signatures, automated legal disclaimers, and promotional footers. The pipeline
will employ a sophisticated cleaning module built upon the official `regex`
crate to identify and remove these artifacts.

The `regex` crate is chosen for its guaranteed linear-time performance
(worst-case $O(m \times n)$ complexity), which prevents catastrophic
backtracking on complex patterns, and its robust, built-in support for Unicode
properties (`\p{...}`), which is essential for correctly processing
international emails.

The cleaning module will apply a sequence of regular expressions to remove:

- **Reply Chains:** Lines prefixed with `>` or patterns like
    `On <Date>, <Person> wrote:`.

- **Signatures:** Common signature delimiters like `--` and blocks of text
    containing phone numbers, addresses, and job titles.

- **Disclaimers:** Boilerplate legal text, often containing phrases like
    "confidentiality notice" or "do not disseminate."

To optimize performance, all regular expressions will be compiled once at
application startup and stored for reuse. The `std::sync::LazyLock` pattern is
the recommended approach for achieving this, as it ensures thread-safe,
on-demand compilation without cluttering initialization logic.[^16]

It is critical to recognize that this cleaning process is inherently heuristic.
Email formats vary immensely, and no static set of rules will ever be perfect.
This reality dictates an important architectural decision: the regex patterns
should not be hardcoded. Instead, they should be treated as a configuration,
loaded from an external source (e.g., a database or a configuration file). This
allows the rules to be refined and updated over time in response to new noise
patterns observed in production, without requiring a full redeployment of the
pipeline.

### Final Normalization

After the primary cleaning operations, a final normalization pass will prepare
the text for the embedding models. This includes standardizing whitespace,
converting the text to lowercase, and potentially handling other special
characters as needed. While the final tokenization is handled by the specific
tokenizers associated with the SBERT and ColBERT models, this pre-normalization
ensures a canonical input format.

## Stage 3: A Meta-Embedding Framework with ColBERT and SBERT

This stage is the intellectual core of the pipeline, where purified text is
transformed into rich, quantitative feature representations. The design employs
a novel "meta-embedding" strategy that utilizes two complementary types of
transformer-based models, SBERT and ColBERT, to capture both broad semantic
meaning and fine-grained token-level relationships.

### The Duality of Semantic Representation

SBERT and ColBERT represent two different philosophies in semantic
representation, and this pipeline leverages the strengths of both.

- **SBERT (Sentence-BERT):** This model is a bi-encoder architecture. It
    processes a piece of text and outputs a single, fixed-size embedding vector
    (e.g., 768 dimensions) that represents the semantic meaning of the entire
    text. The key advantage of SBERT is its efficiency at scale. Document
    embeddings can be pre-computed and indexed in a vector database, allowing
    for extremely fast approximate nearest neighbor (ANN) searches. This makes
    SBERT ideal for first-pass retrieval and broad similarity comparisons
    across millions of documents.

- **ColBERT (Contextualized Late Interaction over BERT):** ColBERT operates
    on a fundamentally different principle. It also uses a BERT-based encoder,
    but critically, it *does not* pool the output into a single vector.
    Instead, it preserves the contextualized embedding for every token in the
    document. A similarity score between two documents is computed at query
    time via a "late interaction" mechanism called MaxSim. For each token in
    the query document, the model finds the most similar token in the target
    document (maximum cosine similarity). These maximum scores are then summed
    to produce a final, highly granular relevance score. This process is
    computationally intensive but provides a far more precise and nuanced
    measure of similarity than a single vector comparison.

### Proposed Meta-Embedding Architecture

For each email, the pipeline will compute and store a meta-embedding object
containing both representations:

1. `sbert_embedding`: A single, dense vector (e.g., `[f32; 768]`), sent inline
    in the `MetaEmbedding` message.

2. `colbert_embeddings`: A variable-length list of dense vectors (e.g.,
    `Vec<[f32; 128]>`), one for each token. This large payload is **not** sent
    over IPC. Instead, it is written to a high-performance blob store (e.g.,
    S3/MinIO) or a memory-mapped file, and a `BlobRef` to its location is
    included in the `MetaEmbedding` message.

This dual representation provides a powerful mechanism for explainability. When
the system determines two emails are similar, the underlying ColBERT
token-level scores can be inspected. This allows the system to highlight the
specific words and phrases that contributed most to the similarity score,
transforming a "black box" similarity score into an interpretable result. For
example, it can show that "quarterly results" in one email aligns strongly with
"financial performance for Q3" in another, providing a clear justification for
their grouping.

### Implementation in Rust

The foundation for implementing these models in Rust is the `rust-bert` crate,
which provides access to the underlying transformer architectures and
pre-trained weights. For SBERT, several community crates like `sbert` and
`rust-sbert` build directly on `rust-bert` to provide a convenient API for
generating sentence embeddings.

A production-ready, off-the-shelf implementation of ColBERT's late-interaction
mechanism in Rust is not readily available. Therefore, this component will
require custom implementation. The engineering team will need to use
`rust-bert` to load a pre-trained BERT model, configure it to output the final
hidden states for all tokens (i.e., disable the final pooling layer), and then
implement the MaxSim scoring logic in Rust. This represents a significant but
achievable engineering task that is central to the pipeline's success.

| **Feature**                       | **SBERT (Bi-Encoder)**                              | **ColBERT (Late Interaction)**                               |
| --------------------------------- | --------------------------------------------------- | ------------------------------------------------------------ |
| **Representation**                | Single vector per document                          | Bag of token vectors per document                            |
| **Primary Use Case**              | Fast, large-scale candidate retrieval (ANN search)  | High-precision re-ranking and pairwise scoring               |
| **Computational Cost (Indexing)** | Moderate (one forward pass per document)            | Moderate (one forward pass per document)                     |
| **Computational Cost (Querying)** | Very Low ($O(1)$ with ANN index)                    | High ($O(q \times d)$ where q, d are token counts)           |
| **Storage Cost**                  | Low (one vector per document)                       | High (many vectors per document, stored out-of-band)         |
| **Key Advantage**                 | Scalability and speed for coarse-grained similarity | High precision and explainability for fine-grained relevance |

## Stage 4: Dynamic, Density-Based Clustering with FISHDBC

### Rationale for FISHDBC

The pipeline will employ FISHDBC for clustering due to its unique combination
of features that are exceptionally well-suited to the challenges of email data.
Unlike traditional algorithms like K-Means, which require a fixed number of
clusters and assume spherical cluster shapes, FISHDBC offers a more powerful
and flexible approach.

- **Flexible Distance Metric:** FISHDBC is designed to work with any
    arbitrary, user-defined dissimilarity function. This is a mandatory
    requirement for our pipeline, as it allows us to directly integrate the
    custom, high-precision distance metric derived from our ColBERT
    meta-embeddings.

- **Incremental and Scalable:** Email data arrives in a continuous stream.
    FISHDBC is an incremental algorithm that can add new data points to an
    existing clustering structure without needing to reprocess the entire
    dataset.[^17] It achieves scalability by using an HNSW (Hierarchical
    Navigable Small World) graph for efficient approximate nearest neighbor
    search, avoiding the prohibitive $O(n^2)$ complexity of naive density-based
    methods.

- **Density-Based:** As an evolution of the DBSCAN family of algorithms,
    FISHDBC identifies clusters as dense regions of data, allowing it to
    discover clusters of arbitrary shapes. Crucially, it can also identify and
    label outliers as "noise"---a vital feature for isolating standalone emails
    that do not belong to any coherent topic or conversation.

- **Hierarchical:** FISHDBC produces a cluster hierarchy, which can be
    represented as a `condensed_tree`. This allows an analyst to explore the
    data at different levels of granularity, from broad, high-level topics down
    to specific, tightly-focused sub-conversations.

### Designing the Custom Distance Metric

The integration between the meta-embedding framework and FISHDBC occurs at the
distance metric. A naive implementation would compute the expensive ColBERT
score between all pairs of points, which is computationally infeasible. The
design will therefore use a hybrid, multi-stage approach:

1. **Candidate Selection (SBERT):** The HNSW index, which is the core of
    FISHDBC's scalable neighbor search, will be built using the efficient SBERT
    embeddings received in the `MetaEmbedding` message. When a new email
    arrives, this index rapidly identifies a small set of candidate similar
    documents.

2. **Precise Scoring (ColBERT):** The `Clusterer` service then invokes a local
    or co-located "ColBERT Scorer" service. It passes the `EmailId` of the new
    email and a list of `EmailId`s for the candidates. The scorer service is
    responsible for fetching the full ColBERT token embeddings for these emails
    from the shared embedding store (e.g., a memory-mapped file on a shared
    volume for maximum performance, using a crate like `memmap2`). It then
    computes the high-precision MaxSim score for each candidate pair.

3. **Distance Calculation:** The custom dissimilarity function provided to
    FISHDBC, `d(email_A, email_B)`, will be defined based on the returned
    ColBERT score, for example, as $1 / (1 + Score_{ColBERT}(A, B))$.

This multi-stage process combines the speed of SBERT for broad searching with
the precision of ColBERT for final scoring, creating a distance metric that is
both highly accurate and computationally tractable.

### Interpreting FISHDBC Output

The output of the FISHDBC algorithm provides rich information about the data's
structure. The primary outputs are the cluster `labels` (assigning each email
to a cluster), `probs` (a stability score indicating how strongly each point
belongs to its cluster), and the `condensed_tree` representing the hierarchy.
These outputs are packaged into `ClusterAssignment` messages and passed
downstream to the knowledge graph.

## Stage 5: Structuring Intelligence for the Event-Centric Knowledge Graph

### From Clusters to Structured Events

The final stage of the pipeline translates the semi-structured outputs of the
clustering engine into a fully structured, queryable knowledge graph. A cluster
identified by FISHDBC represents a latent topic or a coherent conversation; the
goal of this stage is to make the components of that event explicit.

The proposed workflow is as follows:

1. For each stable cluster, the collection of emails within it is treated as a
    single, topic-specific corpus.

2. Named Entity Recognition (NER) is performed on this corpus to extract key
    entities such as `PERSON`, `ORGANIZATION`, `DATE`, `LOCATION`, and project
    codenames. The `rust-bert` library provides pre-trained models and
    pipelines for this task.

3. Relationship extraction techniques are then applied to identify the
    connections between these entities. This can range from pattern-based
    methods to more advanced models that identify subject-predicate-object
    triples.

The token-level alignments from ColBERT can be a powerful tool here. For
example, by comparing a cluster's text to a set of canonical "event trigger"
templates (e.g., "meeting between PERSON and PERSON about TOPIC"), the ColBERT
scores can effectively highlight the specific text spans that fill these slots,
bootstrapping the information extraction process.

### Knowledge Graph Schema and Population

A simple but effective schema will be used to model the data in the knowledge
graph:

- **Nodes:** `Email`, `Person`, `Organization`, `Date`, `TopicCluster`

- **Edges:**

  - `SENT_BY` (Person $\rightarrow$ Email)

  - `RECEIVED_BY` (Person $\rightarrow$ Email)

  - `MENTIONS` (Email $\rightarrow$ Entity)

  - `BELONGS_TO_CLUSTER` (Email $\rightarrow$ TopicCluster)

  - `CLUSTER_CONTAINS` (TopicCluster $\rightarrow$ Entity)

The pipeline's final output will be a stream of `KgTriple` messages, ready for
ingestion into a graph database. This creates a symbiotic analytical
environment where the knowledge graph provides the structured, entity-level
view, while the clustering system provides the thematic context.

## Closing the Loop: IMAP Integration for Cluster Visualization

To make the generated intelligence actionable within standard email clients, a
final pipeline stage will write cluster information back to the source IMAP
server. This allows users to see which emails belong to which cluster directly
in their mail client.

### Strategy: IMAP Keywords and Gmail Labels

The primary mechanism for this write-back is **IMAP keywords** (also known as
custom flags), a feature defined in RFC 3501.

- **IMAP Keywords:** For each message, the IMAP writer service will apply a
    keyword corresponding to its assigned cluster ID, for example:
    `LIMELA.C12345`. These keywords must be "atoms" (containing no special
    characters) and should not start with a backslash (system flags) or `$`
    (registered keywords). Before attempting to set a custom keyword, the
    client MUST check the server's `PERMANENTFLAGS` capability response to
    ensure it contains the `\*` flag, which indicates that the server allows
    the creation of arbitrary newkey words.

- **Gmail-Specific Labels:** When the IMAP server is identified as Gmail (by
    checking for the `X-GM-EXT-1` capability), the system can additionally
    apply a native Gmail label using the `X-GM-LABELS` extension.[^19] This
    provides a more user-friendly experience within the Gmail interface. The
    command would be `STORE <uid> +X-GM-LABELS (Limela/Cluster/12345)`.

- **What to Avoid:** This system will **not** use the `IMAP METADATA`
    extension, as it is designed for mailbox-level annotations, not per-message
    tags. Critically, messages will **never** be rewritten to inject custom
    headers, as this would change message UIDs and break client synchronization.

### Efficient Synchronization and Real-Time Updates

The IMAP connector must operate efficiently and respond to changes in near
real-time.

- **Push Notifications (`IDLE`):** To discover new mail without constant
    polling, the IMAP ingestor will use the `IDLE` command (RFC 2177).[^20] This
    allows the server to push notifications to the client as soon as new
    messages arrive, triggering the processing pipeline.

- **Efficient Sync (`QRESYNC`):** For synchronizing message state (including
    custom flags), the connector will leverage the `QRESYNC` extension (RFC
    7162), which obsoletes the older `CONDSTORE` extension.[^22] This allows the
    client to fetch only the changes that have occurred since its last known
    state, dramatically reducing bandwidth and round-trips.

### Implementation with `async-imap`

The `async-imap` crate is the recommended choice for building the IMAP
connector service.[^24] The "IMAP writer" component will consume
`ClusterAssignment` messages from the pipeline, connect to the appropriate IMAP
server, and issue a `UID STORE <uid> +FLAGS.SILENT (LIMELA.C12345)` command to
apply the cluster tag without generating unnecessary server responses.

## Operationalizing the Pipeline: Concurrency, Orchestration, and Adaptation

### Execution Model: `rayon` vs. `tokio`

The system employs a dual-mode concurrency model tailored to the nature of the
work:

- **Intra-Node Parallelism (`rayon`):** For CPU-bound, data-parallel
    workloads within a single service (e.g., parsing, cleaning, or embedding a
    batch of emails), the `rayon` crate is used. Its `.par_iter()` API provides
    a simple and highly efficient way to saturate all available CPU cores.

- **Inter-Node Concurrency (`tokio`):** For I/O-bound tasks involving network
    or file system access (e.g., handling gRPC requests, reading from S3), the
    `tokio` asynchronous runtime is used. This is the standard for building
    networked services in Rust.

### Orchestration, DAG Modeling, and Deployment

A heavy, general-purpose DAG orchestrator is ill-suited for this low-latency,
streaming workload. Instead, a minimal, bespoke scheduler or "thin coordinator"
is recommended.

- **DAG Modeling with `daggy`:** The pipeline's structure will be defined and
    validated using the `daggy` crate.[^25] As a wrapper around
    `petgraph`, `daggy` provides a convenient API for building graph structures
    while enforcing acyclicity at insertion time, which prevents
    misconfiguration.[^25]

- **Execution:** The thin coordinator will use the underlying `petgraph`
    graph from `daggy` to perform a topological sort
    (`petgraph::algo::toposort`), determining the correct execution order for
    the stages. It will then manage the data flow between the running services.

The architecture supports several deployment topologies:

1. **Development (Single-Process):** All services run in a single binary, with
    gRPC stubs replaced by in-process `tokio::mpsc` channels for
    zero-serialization communication.

2. **Single-Host (Multi-Process):** Services run as separate processes on one
    machine, using Unix Domain Sockets for the data plane to bypass the network
    stack and mTLS over loopback for the control plane.

3. **Distributed:** Services are deployed as independent microservices (e.g.,
    in containers). Communication occurs over the network via gRPC with mTLS.
    This allows for independent scaling, such as provisioning powerful GPU
    nodes for the `Embedder` service while running the `Purifier` on
    CPU-optimized instances.

### Observability with OpenTelemetry

Comprehensive observability is a first-class concern. The system will be
instrumented using the **OpenTelemetry** standard. The
`opentelemetry`, `opentelemetry-otlp`, and `tonic-tracing-opentelemetry` crates
will be used to:

- Propagate a `trace_id` across all stages via gRPC metadata, providing a
    complete, end-to-end view of each email's journey through the pipeline.

- Emit traces, metrics, and logs to a compatible backend for monitoring,
    alerting, and performance analysis.

### Concept Drift Detection with ADWIN

All machine learning systems deployed in real-world environments are
susceptible to concept drift, where the statistical properties of the input
data change over time, degrading model performance.[^18] To make the pipeline
proactively maintainable, it will incorporate a drift detection module based on
the ADWIN (ADaptive WINdowing) algorithm.

This module will be implemented as a separate sidecar service that subscribes
to a stream of `DriftMetric` events emitted from various pipeline stages. ADWIN
is an algorithm that monitors a stream of real-valued data and signals when its
distribution changes significantly, without requiring pre-set window sizes.

The service will monitor key health metrics:

- **Cluster Stability:** The average persistence probability (`probability`)
    from `ClusterAssignment` messages.

- **Noise Ratio:** The percentage of incoming emails classified as noise by
    FISHDBC.

- **Text Properties:** Simple metrics like average email length after
    purification.

When ADWIN detects a statistically significant change, it will trigger an
alert. This serves as an early warning for operators that the system's
performance may be degrading and that components---such as the cleaning regexes
or the embedding models---may require review and updating.

## Implementation Roadmap and Recommended Crate Ecosystem

### Phased Implementation Plan

A phased approach is recommended to manage complexity and deliver value
incrementally.

1. **Phase 1: Core Pipeline Harness.** Implement the end-to-end data flow with
    mock components for each stage. Define the Protobuf schemas and gRPC
    services using `prost-build` and `tonic-build`.[^7] Set up the `rayon`-based
    parallel processing framework within each service.

2. **Phase 2: Normalization and Purification.** Integrate the selected email
    parsing (`mail-parser`) and HTML conversion (`html2text`) crates. Develop
    and rigorously test the initial library of cleaning regular expressions.

3. **Phase 3: Meta-Embedding Integration.** Integrate a pre-trained SBERT
    model using `rust-bert`. Implement the custom ColBERT logic for extracting
    token-level embeddings and writing them to a blob store (e.g., MinIO).

4. **Phase 4: Clustering and KG Integration.** Connect the pipeline to a
    FISHDBC instance. Implement the hybrid SBERT/ColBERT distance metric,
    including the local ColBERT scorer service that reads embeddings from the
    blob store. Develop the NER and relationship extraction modules.

5. **Phase 5: Operationalization.** Implement the ADWIN-based drift detection
    sidecar service. Instrument all services with OpenTelemetry tracing. Deploy
    the complete pipeline to a staging environment for end-to-end testing.

A critical consideration for Phases 4 and 5 is the availability of mature Rust
crates for FISHDBC and ADWIN. The canonical implementation of FISHDBC is in
Python , and common ADWIN implementations are in Java or Python. The
engineering team must make a strategic decision: either implement these
algorithms from scratch in Rust or use a language interop library like PyO3 to
call the existing Python implementations.

### Recommended Crate Ecosystem

The following table summarizes the recommended Rust crates for each major
component of the pipeline.

| **Stage / Functionality**   | **Recommended Crate(s)**                                  | **Rationale & Key Sources**                                                                                                                                         |
| --------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **MIME Parsing**            | `mail-parser`                                             | A fast, robust, and high-level library for parsing complex email structures.                                                                                        |
| **HTML Conversion**         | `html2text`                                               | Uses the browser-grade `html5ever` parser for maximum fidelity in converting HTML to semantically meaningful plain text, which is critical for model input quality. |
| **Regex Processing**        | `regex`                                                   | The official, high-performance regex engine for Rust. Its guaranteed linear-time complexity and strong Unicode support are essential for production use.            |
| **Parallelism (CPU)**       | `rayon`                                                   | The de facto standard for data-parallelism in Rust. Its simple `par_iter` API provides an ergonomic and highly efficient way to parallelize CPU-bound workloads.    |
| **Async Runtime (I/O)**     | `tokio`                                                   | The industry standard for building high-performance, asynchronous network services in Rust.                                                                         |
| **gRPC & Protobuf**         | `tonic`, `prost`                                          | The canonical stack for gRPC in Rust, built on `hyper` and `tokio` for high performance.                                                                            |
| **High-Perf IPC (Opt.)**    | `capnp`, `capnp-rpc`                                      | Provides a zero-copy serialization and capability-based RPC system for surgically optimizing performance-critical communication paths.                              |
| **Transformers/Embeddings** | `rust-bert`, `sbert`                                      | `rust-bert` provides the foundational access to transformer models. `sbert` offers a convenient, high-level API specifically for generating sentence embeddings.    |
| **Blob Storage Client**     | `rust-s3` or `minio-rs`                                   | Mature clients for interacting with S3-compatible object stores like MinIO.                                                                                         |
| **Memory-Mapped Files**     | `memmap2`                                                 | A cross-platform library for using memory-mapped files, ideal for zero-copy access to ColBERT embeddings by a co-located scorer service.                            |
| **Idempotency Hashing**     | `blake3`                                                  | An extremely fast and secure cryptographic hash function, ideal for generating deterministic `EmailId`s.                                                            |
| **Observability**           | `opentelemetry`, `tracing`, `tonic-tracing-opentelemetry` | The standard ecosystem for implementing distributed tracing and metrics.                                                                                            |
| **DAG Modeling**            | `daggy`                                                   | A safe and convenient wrapper around `petgraph` for defining and validating the pipeline's DAG structure. [^25]                                                     |
| **IMAP Connector**          | `async-imap`                                              | A mature, asynchronous client for interacting with IMAP servers, supporting extensions like `IDLE` and `QRESYNC`. [^24]                                             |
| **Clustering**              | `flexible-clustering` (Python) via PyO3, or custom impl.  | The reference implementation for FISHDBC is in Python. A language interop layer is the fastest path to integration.                                                 |
| **Drift Detection**         | Custom implementation                                     | A mature Rust crate for ADWIN is not readily available. The algorithm must be implemented based on the specifications in the relevant academic papers.              |

## Works cited

[^1]: rayon-rs/rayon - A data parallelism library for Rust - GitHub, accessed
      on 22 October 2025, <https://github.com/rayon-rs/rayon>

[^2]: Parallel Processing in Rust - Medium, accessed on 22 October 2025,
      <https://kartik-chauhan.medium.com/parallel-processing-in-rust-d8a7f4a6e32f>

[^3]: (PDF) FISHDBC: Flexible, Incremental, Scalable, Hierarchical
      Density-Based Clustering for Arbitrary Data and Distance - ResearchGate,
      accessed on 22 October 2025,
      <https://www.researchgate.net/publication/336602540_FISHDBC_Flexible_Incremental_Scalable_Hierarchical_Density-Based_Clustering_for_Arbitrary_Data_and_Distance>

[^4]: matteodellamico/flexible-clustering: Clustering for arbitrary data and
      dissimilarity function, accessed on 22 October 2025,
      <https://github.com/matteodellamico/flexible-clustering>

[^5]: mime - Keywords - crates.io: Rust Package Registry, accessed on 22
      October 2025, <https://crates.io/keywords/mime>

[^6]: mail-parser - Rust Package Registry - Crates.io, accessed on 22 October
      2025, <https://crates.io/crates/mail-parser>

[^7]: Comparing 13 Rust Crates for Extracting Text from HTML - Evan Schwartz,
      accessed on 22 October 2025,
      <https://emschwartz.me/comparing-13-rust-crates-for-extracting-text-from-html/>

[^8]: html2text - crates.io: Rust Package Registry, accessed on 22 October
      2025, <https://crates.io/crates/html2text>

[^9]: html2text - crates.io: Rust Package Registry, accessed on 22 October
      2025, <https://crates.io/crates/html2text/dependencies>

[^10]: nanohtml2text --- Rust utility // Lib.rs, accessed on 22 October 2025,
       <https://lib.rs/crates/nanohtml2text>

[^11]: regex - crates.io: Rust Package Registry, accessed on 22 October 2025,
       <https://crates.io/crates/regex>

[^12]: regex - Rust - Docs.rs, accessed on 22 October 2025,
       <https://docs.rs/regex/latest/regex/>

[^13]: Decoding Sentence-BERT | Continuum Labs, accessed on 22 October 2025,
       <https://training.continuumlabs.ai/knowledge/vector-databases/decoding-sentence-bert>

[^14]: Understanding ColBERT: What is New Comparing with Normal Semantic Search
       - Medium, accessed on 22 October 2025,
       <https://medium.com/@liu.peng.uppsala/understanding-colbert-what-is-new-comparing-with-normal-semantic-search-6dc285311a18>

[^15]: ColBERT --- A Late Interaction Model For Semantic Search | by Zachariah
       Zhang | Medium, accessed on 22 October 2025,
       <https://medium.com/@zz1409/colbert-a-late-interaction-model-for-semantic-search-da00f052d30e>

[^16]: Can Semantic Search be more interpretable? COLBERT, SPLADE might be the
       answer but is it enough?, accessed on 22 October 2025,
       <http://musingsaboutlibrarianship.blogspot.com/2024/06/can-semantic-search-be-more.html>

[^17]: rust-bert - Crates.io, accessed on 22 October 2025,
       <https://crates.io/crates/rust-bert/reverse_dependencies>

[^18]: rust\_bert::pipelines - Rust - Docs.rs, accessed on 22 October 2025,
       <https://docs.rs/rust-bert/latest/rust_bert/pipelines/index.html>

[^19]: cpcdoy/rust-sbert: Rust port of sentence-transformers
       (https://github.com/UKPLab/sentence-transformers) - GitHub, accessed on
       22 October 2025, <https://github.com/cpcdoy/rust-sbert>

[^20]: [1910.07283] FISHDBC: Flexible, Incremental, Scalable, Hierarchical
       Density-Based Clustering for Arbitrary Data and Distance - arXiv,
       accessed on 22 October 2025, <https://arxiv.org/abs/1910.07283>

[^21]: A Guide to the DBSCAN Clustering Algorithm - DataCamp, accessed on 22
       October 2025,
       <https://www.datacamp.com/tutorial/dbscan-clustering-algorithm>

[^22]: Data Parallelism - Rust Cookbook, accessed on 22 October 2025,
       <https://rust-lang-nursery.github.io/rust-cookbook/concurrency/parallel.html>

[^23]: parallel\_stream - Rust - Docs.rs, accessed on 22 October 2025,
       <https://docs.rs/parallel-stream>

[^24]: Dynamic Serialization with Protobuf and Embedded Rust - A Calustra- Eloy
       Coto, accessed on 22 October 2025,
       <https://acalustra.com/dynamic-serialization-with-protobuf-on-embedded-rust.html>

[^25]: Rust Generated Code Guide | Protocol Buffers Documentation, accessed on
       22 October 2025, <https://protobuf.dev/reference/rust/rust-generated/>

[^26]: An Online, Adaptive and Unsupervised Regression Framework with Drift
       Detection for Label Scarcity Contexts - arXiv, accessed on 22 October
       2025, <https://arxiv.org/html/2312.07682v1>

[^27]: Scalable Detection of Concept Drifts on Data Streams with Parallel
       Adaptive Windowing, accessed on 22 October 2025,
       <https://www.dfki.de/fileadmin/user_upload/import/9720_grulich-Scalable-Detection-of-Concept-Drifts-on-Data-Streams-with-Parallel-Adaptive-Windowing.pdf>

[^28]: skmultiflow.drift\_detection.ADWIN - scikit-multiflow's documentation! -
       Read the Docs, accessed on 22 October 2025,
       <https://scikit-multiflow.readthedocs.io/en/stable/api/generated/skmultiflow.drift_detection.ADWIN.html>

[^29]: Automated Data Quality Monitoring with ADWIN2 - Jefferson Lab Indico,
       accessed on 22 October 2025,
       <https://indico.jlab.org/event/419/contributions/7651/attachments/6355/8425/Farhat-ADWIN-20210113.pdf>
