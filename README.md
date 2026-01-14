# Limela ✨

> **isiLimela** — the Zulu name for the Pleiades, the constellation that
> signals the time of planting and renewal in Bantu cultures.

Limela finds the constellations in your emails, bringing order to your world.

---

## What is Limela?

Limela is an email intelligence pipeline that transforms your chaotic inbox
into a structured knowledge graph. It reads your emails, understands their
meaning, discovers patterns and conversations, and helps you see the bigger
picture — the constellations hidden in your daily communication.

Think of it as an astronomer for your inbox: while you see individual stars
(emails), Limela reveals the constellations (topics, threads, relationships)
that connect them.

## Current Status: Work in Progress 🚧

Limela is in active development. We have comprehensive architectural plans and
are building the foundation. Check out [`docs/roadmap.md`](docs/roadmap.md) to
see where we're headed.

## The Vision: Five Stages to Clarity

Limela processes emails through a sophisticated pipeline:

1. **Normalization** — Parse MIME structures, extract headers, handle
   attachments
2. **Purification** — Convert HTML to clean text, remove noise (signatures,
   disclaimers, reply chains)
3. **Meta-Embedding** — Generate semantic representations using transformer
   models (SBERT + ColBERT)
4. **Clustering** — Discover thematic groups and conversation threads
   automatically using density-based clustering
5. **Knowledge Graph** — Build a queryable graph of people, topics, threads,
   and their relationships

The result? Your emails become a navigable constellation map, with patterns
and connections revealed.

## Design Goals

- **High-performance processing** — Built in Rust for speed and reliability
- **Privacy by design** — Knowledge graph will store only metadata and
  encrypted pointers; email bodies remain in your mail store
- **IMAP integration** — Tag emails in your existing mail client based on
  discovered clusters
- **Semantic search** — Find emails by meaning, not just keywords
- **Conversation threading** — Automatic discovery of related emails across
  time and topics
- **Streaming updates** — Real-time processing as new emails arrive

Note: Limela is in early development. These are architectural goals, not yet
implemented features.

## Getting Started (For Developers)

Limela requires Rust (Edition 2024) and uses a strict code quality standard.

### Prerequisites

- Rust toolchain 1.85+ (MSRV: 1.85)
- Make (for convenience commands)

### Quick Start

```bash
# Build the project
make build

# Run tests
make test

# Check code quality
make lint

# Format code
make fmt
```

For detailed development guidelines and coding standards, see the
documentation in the `docs/` directory.

## Related Projects

Limela is built on powerful foundations:

- **[Chutoro](https://github.com/leynos/chutoro)** — Our FISHDBC (Flexible,
  Incremental, Scalable, Hierarchical Density-Based Clustering) implementation
  that forms the core clustering engine of Limela
- **[Telephone](https://github.com/leynos/telephone)** — Our GPU-accelerated
  neurosymbolic reasoning playground for large language models that will power
  Limela's intelligence

## Documentation

- **[Roadmap](docs/roadmap.md)** — Development phases and current status
- **[Pipeline Design](docs/limela-pipeline-design.md)** — Comprehensive
  architectural documentation
- **[ADR-001](docs/adr-001-eckg-v0-implementation-with-oxigraph-triple-store.md)**
  — Knowledge graph implementation decision
- **[Testing Guide](docs/rust-testing-with-rstest-fixtures.md)** — Testing
  strategies and patterns
- **[Documentation Style Guide](docs/documentation-style-guide.md)** — Writing
  standards for docs

## Why Limela?

Email is one of our most valuable knowledge sources, but it's trapped in
chronological chaos. We scan the same threads repeatedly, lose track of
important conversations, and struggle to see patterns across time.

Limela brings the power of modern NLP, graph theory, and clustering algorithms
to your inbox — helping you work with your email as a knowledge base, not a
to-do list.

The constellations have always been there. Limela helps you see them.

## Licence

ISC Licence — See [LICENSE](LICENSE) for details.

## Developed By

**[df12 Productions](https://df12.studio)** ✨

---

> *"In the same way the Pleiades signal a time of renewal and growth, Limela
> brings new understanding to your communication."*
