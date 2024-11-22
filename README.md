# OMAR - The Ollama Model Report Tool

A command-line utility that generates usage reports for your Ollama models, helping you track model sizes and usage patterns.

## Features

- Lists all installed Ollama models
- Tracks model usage statistics:
  - Last used timestamp
  - Usage count
  - Model size
- Supports custom model directories via `OLLAMA_MODELS` environment variable
- Cross-platform support (macOS and Windows)

## Installation

### Prerequisites

- Rust toolchain (install from [rustup.rs](https://rustup.rs))
- Ollama installed on your system

### Building from Source

```bash
git clone https://github.com/yourusername/ollama-model-report.git
cd ollama-model-report
cargo build --release
```

The compiled binary will be available in `target/release/ollama-model-report`

## Usage

Simply run the binary in your terminal:

```bash
./ollama-model-report
```

The tool will automatically:
1. Scan your Ollama models directory
2. Parse model manifests
3. Analyze usage logs
4. Generate a report showing model usage statistics

### Environment Variables

- `OLLAMA_MODELS`: Set this to override the default Ollama models directory path

## Dependencies

- `serde`, `serde_json`: For JSON serialization/deserialization
- `chrono`: For timestamp handling
- `glob`: For file pattern matching
- `dirs`: For finding user directories
- `anyhow`: For error handling
- `clap`: For command-line argument parsing

## License

[Insert your chosen license here]

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
