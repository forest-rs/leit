# Leit Benchmark Specification

**Status:** 📋 Specification  
**Phase:** 1  
**Component:** Performance Benchmarking  
**Inspired By:** FTSB (Full Text Search Benchmark)

---

## 1. Overview and Purpose

`leit_benchmark` is a standalone performance characterization tool for the Leit search engine library. It provides standardized benchmarking capabilities to measure indexing throughput, query latency, and overall system performance under various workloads.

### Core Responsibilities

- **Dataset Generation:** Create or load test datasets compatible with Leit fixtures
- **Indexing Benchmarking:** Measure document indexing throughput and memory usage
- **Query Benchmarking:** Execute query workloads and measure latency distributions
- **Metrics Reporting:** Output standardized metrics for performance tracking
- **Comparison Support:** Enable performance regression detection between versions

### Design Philosophy

- **FTSB-Inspired:** Follow the Full Text Search Benchmark methodology for metrics collection
- **Deterministic:** All benchmarks must be reproducible across runs
- **Isolated:** Benchmark runs must not interfere with each other
- **Minimal Dependencies:** Standalone binary that doesn't require external services
- **Standard Formats:** Output JSON for machine parsing and human-readable summaries

### Non-Goals

- Correctness testing (handled by `leit_testing` infrastructure)
- Distributed system benchmarking
- Production monitoring or alerting
- Load testing for web services
- Multi-language benchmarking (English-only focus)

---

## 2. Data Formats

### 2.1 Dataset Structure

Datasets are JSON files compatible with Leit testing fixtures:

```json
{
  "meta": {
    "name": "wiki-abstract-small",
    "description": "Wikipedia abstracts subset",
    "document_count": 1000,
    "created_at": "2024-01-15T10:00:00Z"
  },
  "documents": [
    {
      "id": "doc-001",
      "title": "Example Document",
      "body": "Document content goes here...",
      "fields": {
        "category": "technology",
        "timestamp": "2024-01-15T10:00:00Z"
      }
    }
  ]
}
```

### 2.2 Query Workload Structure

Query workloads define test scenarios:

```json
{
  "meta": {
    "name": "wiki-abstract-queries",
    "dataset": "wiki-abstract-small",
    "description": "Standard query mix for Wikipedia abstracts"
  },
  "queries": [
    {
      "id": "q-001",
      "query": "machine learning algorithms",
      "type": "term",
      "expected_results": 10
    },
    {
      "id": "q-002",
      "query": "title:neural network",
      "type": "phrase",
      "expected_results": 5
    }
  ]
}
```

### 2.3 Built-in Datasets

The benchmark tool includes generator scripts to create synthetic datasets:

| Dataset | Size | Use Case | Generation Command |
|---------|------|----------|-------------------|
| `wiki-tiny` | 100 docs | Quick validation | `leit-benchmark gen wiki-tiny` |
| `wiki-small` | 1,000 docs | Development | `leit-benchmark gen wiki-small` |
| `wiki-medium` | 10,000 docs | Standard benchmarking | `leit-benchmark gen wiki-medium` |
| `wiki-large` | 100,000 docs | Stress testing | `leit-benchmark gen wiki-large` |
| `ecommerce-small` | 500 products | E-commerce scenario | `leit-benchmark gen ecommerce-small` |
| `ecommerce-medium` | 5,000 products | E-commerce benchmark | `leit-benchmark gen ecommerce-medium` |

---

## 3. Benchmark Scenarios

### 3.1 Wikipedia Abstracts (wiki-abstract)

Simulates encyclopedia search with long-form text:

**Characteristics:**
- Document size: 200-500 words
- Query patterns: informational searches
- Vocabulary: diverse, academic/technical terms
- Result set: 10-100 relevant documents per query

**Workload Profile:**
```json
{
  "scenario": "wiki-abstract",
  "indexing": {
    "batch_size": 100,
    "fields": ["title", "body"],
    "tokenization": "default"
  },
  "queries": {
    "distribution": {
      "single_term": 0.30,
      "multi_term": 0.40,
      "phrase": 0.20,
      "boolean": 0.10
    },
    "complexity": "medium"
  }
}
```

### 3.2 E-Commerce Product Catalog (ecommerce)

Simulates product search with structured attributes:

**Characteristics:**
- Document size: 50-150 words
- Query patterns: navigational + transactional searches
- Vocabulary: product names, categories, specifications
- Result set: 5-50 relevant products per query

**Workload Profile:**
```json
{
  "scenario": "ecommerce",
  "indexing": {
    "batch_size": 50,
    "fields": ["name", "description", "category", "brand"],
    "tokenization": "product",
    "boost_fields": ["name^2", "category^1.5"]
  },
  "queries": {
    "distribution": {
      "single_term": 0.20,
      "multi_term": 0.50,
      "phrase": 0.15,
      "filter": 0.15
    },
    "complexity": "low"
  }
}
```

---

## 4. Metrics Collection

### 4.1 Indexing Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `total_documents` | Number of documents indexed | count |
| `indexing_duration` | Total time to index all documents | ms |
| `throughput` | Documents indexed per second | docs/sec |
| `avg_doc_size` | Average document size in bytes | bytes |
| `peak_memory` | Peak memory usage during indexing | MB |
| `index_size` | Final index size on disk/bytes | MB |

### 4.2 Query Metrics (FTSB-Inspired)

| Metric | Description | Unit |
|--------|-------------|------|
| `total_queries` | Number of queries executed | count |
| `query_duration` | Total time for all queries | ms |
| `throughput` | Queries executed per second | queries/sec |
| `p50_latency` | Median query latency | ms |
| `p90_latency` | 90th percentile latency | ms |
| `p95_latency` | 95th percentile latency | ms |
| `p99_latency` | 99th percentile latency | ms |
| `min_latency` | Minimum query latency | ms |
| `max_latency` | Maximum query latency | ms |
| `avg_results` | Average number of results per query | count |

### 4.3 Resource Metrics

| Metric | Description | Unit |
|--------|-------------|------|
| `cpu_time` | Total CPU time used | ms |
| `memory_usage` | Memory consumption | MB |
| `heap_allocated` | Total heap allocations | MB |
| `page_faults` | Major page faults | count |

---

## 5. CLI Interface Design

### 5.1 Command Structure

```bash
leit-benchmark <COMMAND> [OPTIONS]
```

### 5.2 Available Commands

#### `gen` - Generate Datasets

```bash
leit-benchmark gen <DATASET_TYPE> [OPTIONS]

Options:
  -o, --output <PATH>      Output directory [default: ./benchmark_data]
  -s, --size <SIZE>        Override default size (document count)
  --seed <SEED>            Random seed for reproducibility [default: 42]
  --format <FORMAT>        Output format [default: json] [possible values: json, msgpack]
```

**Examples:**
```bash
# Generate small Wikipedia dataset
leit-benchmark gen wiki-small

# Generate custom-sized dataset
leit-benchmark gen wiki-medium -s 50000 -o ./custom_data

# Generate e-commerce dataset
leit-benchmark gen ecommerce-medium
```

#### `index` - Benchmark Indexing

```bash
leit-benchmark index <DATASET> [OPTIONS]

Options:
  -w, --warmup <RUNS>      Number of warmup runs [default: 1]
  -r, --runs <RUNS>        Number of benchmark runs [default: 3]
  -b, --batch-size <SIZE>  Documents per batch [default: 100]
  -o, --output <PATH>      Output results to file
  --no-memory              Disable memory profiling
  --profile <TYPE>         Profile type [default: throughput] [possible values: throughput, memory, latency]
```

**Examples:**
```bash
# Benchmark indexing with default settings
leit-benchmark index benchmark_data/wiki-medium.json

# Run 5 iterations with batch size 50
leit-benchmark index wiki-medium -r 5 -b 50

# Save results to file
leit-benchmark index wiki-medium -o results/indexing_2024-01-15.json
```

#### `query` - Benchmark Queries

```bash
leit-benchmark query <DATASET> <WORKLOAD> [OPTIONS]

Options:
  -w, --warmup <RUNS>      Number of warmup runs [default: 1]
  -r, --runs <RUNS>        Number of benchmark runs [default: 3]
  -t, --threads <COUNT>    Number of concurrent threads [default: 1]
  -o, --output <PATH>      Output results to file
  --percentiles <LIST>     Percentiles to report [default: 50,90,95,99]
  --duration <SECONDS>     Run for specified duration instead of query count
```

**Examples:**
```bash
# Benchmark queries
leit-benchmark query wiki-medium wiki-queries.json

# Run with 4 concurrent threads for 30 seconds
leit-benchmark query wiki-medium wiki-queries.json -t 4 --duration 30

# Custom percentiles
leit-benchmark query wiki-medium wiki-queries.json --percentiles 50,75,90,95,99,99.9
```

#### `run` - Execute Full Benchmark Suite

```bash
leit-benchmark run <SCENARIO> [OPTIONS]

Options:
  -o, --output <DIR>       Output directory for results [default: ./benchmark_results]
  -f, --format <FORMAT>    Output format [default: both] [possible values: json, human, both]
  --compare <BASELINE>     Compare results against baseline file
  --fail-on-regression     Exit with error if performance regresses
  --threshold <PERCENT>    Regression threshold % [default: 10]
```

**Examples:**
```bash
# Run full Wikipedia benchmark suite
leit-benchmark run wiki-abstract

# Run and compare against baseline
leit-benchmark run ecommerce --compare baseline/ecommerce_2024-01-01.json

# Run with regression detection
leit-benchmark run wiki-abstract --fail-on-regression --threshold 5
```

#### `compare` - Compare Benchmark Results

```bash
leit-benchmark compare <BASELINE> <CURRENT> [OPTIONS]

Options:
  -o, --output <PATH>      Save comparison report to file
  --threshold <PERCENT>    Highlight differences above threshold [default: 5]
  --format <FORMAT>        Report format [default: human] [possible values: human, json, markdown]
```

**Examples:**
```bash
# Compare two result files
leit-benchmark compare baseline/v1.0.json results/v1.1.json

# Generate markdown report
leit-benchmark compare baseline/v1.0.json results/v1.1.json --format markdown -o comparison.md
```

### 5.3 Global Options

```bash
leit-benchmark [GLOBAL_OPTIONS] <COMMAND>

Global Options:
  -v, --verbose            Increase verbosity (can be used multiple times)
  -q, --quiet              Suppress output except errors
  -h, --help               Print help information
  -V, --version            Print version information
```

---

## 6. Output Format

### 6.1 JSON Output (Machine-Parsable)

```json
{
  "meta": {
    "benchmark_version": "1.0.0",
    "leit_version": "0.1.0",
    "timestamp": "2024-01-15T10:30:00Z",
    "hostname": "benchmark-machine",
    "cpu_info": "Apple M1 Max",
    "memory_gb": 64,
    "os": "macOS 14.2"
  },
  "scenario": {
    "name": "wiki-abstract",
    "dataset": "wiki-medium",
    "document_count": 10000,
    "query_count": 100
  },
  "indexing": {
    "runs": [
      {
        "run_id": 1,
        "duration_ms": 2345,
        "throughput_docs_per_sec": 4264.3,
        "peak_memory_mb": 256.5,
        "index_size_mb": 45.2
      },
      {
        "run_id": 2,
        "duration_ms": 2298,
        "throughput_docs_per_sec": 4351.6,
        "peak_memory_mb": 255.8,
        "index_size_mb": 45.2
      }
    ],
    "summary": {
      "avg_throughput": 4307.9,
      "stddev_throughput": 61.6,
      "min_throughput": 4264.3,
      "max_throughput": 4351.6,
      "avg_duration_ms": 2321.5
    }
  },
  "queries": {
    "runs": [
      {
        "run_id": 1,
        "duration_ms": 1245,
        "throughput_queries_per_sec": 80.3,
        "latencies": {
          "p50": 8.2,
          "p90": 15.6,
          "p95": 19.3,
          "p99": 28.7,
          "min": 2.1,
          "max": 45.2
        }
      }
    ],
    "summary": {
      "avg_throughput": 81.2,
      "p50_latency": 8.5,
      "p90_latency": 16.1,
      "p95_latency": 19.8,
      "p99_latency": 29.3
    }
  }
}
```

### 6.2 Human-Readable Output

```
╭─────────────────────────────────────────────────────────────────╮
│                    Leit Benchmark Results                       │
├─────────────────────────────────────────────────────────────────┤
│ Scenario:       wiki-abstract                                   │
│ Dataset:        wiki-medium (10,000 docs)                       │
│ Leit Version:   0.1.0                                           │
│ Timestamp:      2024-01-15 10:30:00 UTC                         │
│ Hardware:       Apple M1 Max, 64GB RAM                          │
╰─────────────────────────────────────────────────────────────────╯

┌─────────────────────────────────────────────────────────────────┐
│ Indexing Performance                                            │
├──────────────────┬──────────────┬──────────────┬───────────────┤
│ Run             │ Duration     │ Throughput   │ Memory (MB)   │
├──────────────────┼──────────────┼──────────────┼───────────────┤
│ Run 1           │ 2,345 ms     │ 4,264 doc/s   │ 256.5         │
│ Run 2           │ 2,298 ms     │ 4,352 doc/s   │ 255.8         │
│ Run 3           │ 2,312 ms     │ 4,325 doc/s   │ 256.1         │
├──────────────────┼──────────────┼──────────────┼───────────────┤
│ Average         │ 2,318 ms     │ 4,314 doc/s   │ 256.1         │
│ Std Dev         │ 23.5 ms      │ 43.7 doc/s    │ 0.4           │
└──────────────────┴──────────────┴──────────────┴───────────────┘

┌─────────────────────────────────────────────────────────────────┐
│ Query Performance                                               │
├──────────────────┬──────────────┬──────────────┬───────────────┤
│ Run             │ Duration     │ Throughput   │ P99 Latency   │
├──────────────────┼──────────────┼──────────────┼───────────────┤
│ Run 1           │ 1,245 ms     │ 80.3 q/s      │ 28.7 ms       │
│ Run 2           │ 1,228 ms     │ 81.4 q/s      │ 27.9 ms       │
│ Run 3           │ 1,239 ms     │ 80.7 q/s      │ 28.3 ms       │
├──────────────────┼──────────────┼──────────────┼───────────────┤
│ Average         │ 1,237 ms     │ 80.8 q/s      │ 28.3 ms       │
└──────────────────┴──────────────┴──────────────┴───────────────┘

Latency Percentiles (ms):
┌──────────┬───────┬───────┬───────┬───────┬───────┬───────┐
│ Percent  │ P50   │ P75   │ P90   │ P95   │ P99   │ P99.9 │
├──────────┼───────┼───────┼───────┼───────┼───────┼───────┤
│ Latency  │ 8.5   │ 12.3  │ 16.1  │ 19.8  │ 29.3  │ 38.2  │
└──────────┴───────┴───────┴───────┴───────┴───────┴───────┘

Index Size: 45.2 MB
Documents Indexed: 10,000
Queries Executed: 100
Average Results per Query: 23.4
```

### 6.3 Comparison Report

```
Performance Comparison: Baseline vs Current

╭─────────────────────────────────────────────────────────────────╮
│ Indexing Throughput                                             │
├──────────────────┬──────────────┬──────────────┬───────────────┤
│ Metric           │ Baseline     │ Current      │ Change        │
├──────────────────┼──────────────┼──────────────┼───────────────┤
│ Throughput       │ 4,100 doc/s  │ 4,314 doc/s  │ +5.2% ✓       │
│ P99 Latency      │ 32.1 ms      │ 28.3 ms      │ -11.8% ✓      │
│ Memory Usage     │ 265.3 MB     │ 256.1 MB     │ -3.5% ✓       │
│ Index Size       │ 47.8 MB      │ 45.2 MB      │ -5.4% ✓       │
└──────────────────┴──────────────┴──────────────┴───────────────┘

All metrics within threshold ✓
No regressions detected
```

---

## 7. Running Benchmarks

### 7.1 Quick Start

```bash
# 1. Generate test dataset
leit-benchmark gen wiki-small

# 2. Run indexing benchmark
leit-benchmark index benchmark_data/wiki-small.json -o results/indexing.json

# 3. Run query benchmark
leit-benchmark query benchmark_data/wiki-small.json benchmark_data/wiki-queries.json -o results/query.json

# 4. Run full suite
leit-benchmark run wiki-abstract -o results/
```

### 7.2 Reproducible Benchmarks

To ensure reproducible results:

```bash
# Use fixed seed for dataset generation
leit-benchmark gen wiki-medium --seed 42 -o benchmark_data/

# Run benchmarks with fixed parameters
leit-benchmark run wiki-abstract -r 5 -w 2 -o results/

# Document environment
leit-benchmark run wiki-abstract --save-metadata > results/metadata.txt
```

### 7.3 Continuous Integration

Example CI workflow:

```bash
#!/bin/bash
# ci-benchmark.sh

# Generate baseline dataset once
if [ ! -d "benchmark_data" ]; then
  leit-benchmark gen wiki-medium --seed 42 -o benchmark_data/
fi

# Run benchmarks
leit-benchmark run wiki-abstract \
  -o results/ \
  --compare baseline/master.json \
  --fail-on-regression \
  --threshold 10

# Upload results
# ... CI-specific code to upload results to artifact storage
```

---

## 8. Comparing Results

### 8.1 Version Comparison

Compare performance between Leit versions:

```bash
# Benchmark version 1.0
git checkout v1.0.0
leit-benchmark run wiki-abstract -o baseline/v1.0.json

# Benchmark version 1.1
git checkout v1.1.0
leit-benchmark run wiki-abstract -o results/v1.1.json

# Generate comparison
leit-benchmark compare baseline/v1.0.json results/v1.1.json \
  --format markdown -o comparison/v1.0_vs_v1.1.md
```

### 8.2 Regression Detection

Automatically detect performance regressions:

```bash
# This will exit with error code if any metric regresses > 10%
leit-benchmark run wiki-abstract \
  --compare baseline/master.json \
  --fail-on-regression \
  --threshold 10
```

### 8.3 Trend Analysis

Track performance over time:

```bash
#!/bin/bash
# benchmark-history.sh

VERSION=$1
DATE=$(date +%Y%m%d)

# Run benchmark
leit-benchmark run wiki-abstract -o results/${DATE}_${VERSION}.json

# Update history file
echo "${DATE},${VERSION},$(jq '.indexing.summary.avg_throughput' results/${DATE}_${VERSION}.json)" \
  >> history/indexing_throughput.csv
```

---

## 9. Implementation Notes

### 9.1 Performance Considerations

- **Warmup Runs:** Always execute warmup iterations to allow JIT compilation and cache warming
- **Memory Profiling:** Use platform-appropriate tools (e.g., `/proc/self/status` on Linux)
- **Thread Pinning:** For multi-threaded benchmarks, consider CPU affinity
- **I/O Isolation:** Ensure benchmark data is on fast storage (SSD/NVMe)

### 9.2 Statistical Validity

- **Multiple Runs:** Execute each benchmark 3-5 times and report statistics
- **Outlier Detection:** Consider using IQR or z-score to detect and flag outliers
- **Confidence Intervals:** For production benchmarks, calculate 95% confidence intervals

### 9.3 Environment Documentation

Always capture and report:
- CPU model and core count
- Total RAM and available memory
- OS version and kernel version
- Rust compiler version
- Cargo release profile settings
- System load during benchmark

---

## 10. Future Enhancements

Potential future additions to the benchmark tool:

- **Additional Scenarios:** Logs, legal documents, social media feeds
- **Compression Benchmarks:** Measure posting list compression ratios and speed
- **Scoring Algorithm Comparison:** Compare BM25 variants, TF-IDF, etc.
- **Memory Profiling:** Detailed heap allocation profiling
- **Custom Workloads:** Support for user-defined query distributions
- **Distributed Benchmarking:** Multi-node cluster benchmarking (future phases)
