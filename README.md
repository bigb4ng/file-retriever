# Retriever

A rust library for downloading files from the web. You can download multiple files asyncronously.
The library is built on top of [reqwest](https://github.com/seanmonstar/reqwest) and [tokio](https://github.com/tokio-rs/tokio).
It also utilizes [indicatif](https://github.com/console-rs/indicatif) for progress bars.

## Examples
Multiple examples are available in [src/lib.rs](https://github.com/bigb4ng/retriever/blob/main/src/lib.rs)

### Single file download
```rust
use reqwest::Client;
use retriever::{self, RetrieverBuilder};
use tokio::fs::OpenOptions;

#[tokio::main]
async fn main() -> Result<()> {
    let retriever = RetrieverBuilder::new()
        .show_progress(false)
        .workers(1)
        .build();

    let request = Client::new()
        .get("https://example.com")
        .build()
        .unwrap();

    let output_file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open("/tmp/test")
        .await
        .unwrap();

    let _ = retriever.download_file(request, output_file).await;
}
```

### Multiple file download
Example of downloading multiple files can be seen in the [multi_download_example.rs](https://github.com/bigb4ng/retriever/blob/main/examples/multi_download_example.rs)

You can run example with:
```bash
cargo run --example multi_download_example  -- -i ./examples/in/ -o ./examples/out/
```

## Tests
You can execute tests with:
```bash
cargo test --lib
```
which will create fetch files from mock server to `/tmp/` directory
