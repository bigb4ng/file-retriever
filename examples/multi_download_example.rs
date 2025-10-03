use argh::FromArgs;
use file_retriever::{self, RetrieverBuilder};
use reqwest::Client;
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::JoinSet;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(FromArgs)]
/// A simple command line tool to fetch a list of URLs and save them to a files
struct FetchParams {
    /// the "User-Agent" header to use when fetching URLs
    #[argh(
        option,
        short = 'A',
        default = "String::from(\"Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36\")"
    )]
    user_agent: String,

    /// the directory to iterate for links.txt files
    #[argh(option, short = 'i', default = "String::from(\"./examples/in/\")")]
    input_dir: String,

    /// the directory to save the files to
    #[argh(option, short = 'o', default = "String::from(\"./examples/out/\")")]
    output_dir: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let params: Arc<FetchParams> = Arc::new(argh::from_env());
    let retriever = Arc::new(
        RetrieverBuilder::new()
            .show_progress(true)
            .workers(2)
            .build(),
    );
    let mut join_set = JoinSet::new();

    let mut dir_entries = fs::read_dir(&params.input_dir).await?;
    while let Some(entry) = dir_entries.next_entry().await? {
        let dir_entry = Arc::new(entry);
        let file = File::open(dir_entry.path().join("links.txt")).await?;
        let buf_reader = BufReader::new(file);
        let mut lines = buf_reader.lines();

        while let Some(url_string) = lines.next_line().await? {
            let retriever_clone = Arc::clone(&retriever);
            let dir_entry_clone = Arc::clone(&dir_entry);
            let params_clone = Arc::clone(&params);

            join_set.spawn(async move {
                let url = Url::parse(&url_string).expect("URL should be valid");

                // will produce out/example/example.jpg for url https://example.com/a/b/c/example.jpg in out/example/links.txt
                let filename = url
                    .path_segments()
                    .expect("URL must have path segments")
                    .last()
                    .expect("URL must have filename part");
                let path = Path::new(&params_clone.output_dir)
                    .join(dir_entry_clone.file_name())
                    .join(filename);

                fs::create_dir_all(path.parent().unwrap()).await?;

                let file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(path.as_os_str())
                    .await?;
                let req = Client::new()
                    .get(url.as_str())
                    .header("User-Agent", params_clone.user_agent.as_bytes())
                    .build()
                    .expect("request should build");

                retriever_clone.download_file(req, file).await
            });
        }
    }

    while let Some(result) = join_set.join_next().await {
        if let Ok(res) = result {
            if let Err(e) = res {
                eprintln!("{:?}", e)
            }
        }
    }

    println!("Complete!");
    Ok(())
}
