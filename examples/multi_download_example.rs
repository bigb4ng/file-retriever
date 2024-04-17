use argh::FromArgs;
use reqwest::Client;
use retriever::{self, RetrieverBuilder};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs::OpenOptions;
use tokio::task::JoinSet;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn read_lines(filename: &OsStr) -> Vec<String> {
    fs::read_to_string(filename)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

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

    let mut set = JoinSet::new();

    let paths = fs::read_dir(&params.input_dir)?;
    let retriever = Arc::new(
        RetrieverBuilder::new()
            .show_progress(true)
            .workers(10)
            .build(),
    );

    for path in paths {
        let dir_entry = Arc::new(path.unwrap());
        let url_strings = read_lines(dir_entry.path().join("links.txt").as_os_str());

        for url_string in url_strings {
            let retriever_clone = Arc::clone(&retriever);
            let dir_entry_clone = Arc::clone(&dir_entry);
            let params_clone = Arc::clone(&params);

            set.spawn(async move {
                let url = Url::parse(url_string.as_str()).expect("URL should be valid");

                // will produce out/example/example.jpg for url https://example.com/a/b/c/example.jpg in out/example/links.txt
                let path: PathBuf = [
                    OsStr::new(&params_clone.output_dir),
                    dir_entry_clone.file_name().as_os_str(),
                    OsStr::new(
                        url.path_segments()
                            .expect("URL must have path segments")
                            .last()
                            .expect("URL must have filename part"),
                    ),
                ]
                .into_iter()
                .collect();
                if path.exists() {
                    return Ok(());
                }

                std::fs::create_dir_all(path.parent().unwrap().as_os_str())?;

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
                    .unwrap();
                retriever_clone.download_file(req, file).await
            });
        }
    }

    while let Some(result) = set.join_next().await {
        if let Ok(res) = result {
            if let Err(e) = res {
                eprintln!("{:?}", e)
            }
        }
    }

    Ok(())
}
