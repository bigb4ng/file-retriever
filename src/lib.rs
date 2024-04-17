#![deny(unsafe_code)]
#![deny(missing_docs)]

//! Asyncronous download with (optional) progress bar and limited amount of workers.
//!
//! Retriever is based on tokio and reqwest crates dancing together in a beautiful tango.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Request;
use tokio::io::AsyncWriteExt;
use tokio::sync::Semaphore;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Factory which is used to configure the properties of a new Retriever.
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use retriever::RetrieverBuilder;
/// use tokio::fs::OpenOptions;
///
/// #[tokio::main]
/// async fn main() {
///     // build a retriever
///     let retriever = RetrieverBuilder::new()
///         .show_progress(true)
///         .workers(42)
///         .build();
///
///     // open a file to write to
///     let file = OpenOptions::new()
///         .create(true)
///         .write(true)
///         .truncate(true)
///         .open(format!("index.html"))
///         .await
///         .expect("should return file");
///
///     // setup a request to retrieve the file
///     let req = Client::new().get("https://example.com").build().unwrap();
///
///     // download a file
///     let _  = retriever.download_file(req, file).await;
/// }
/// ```
pub struct RetrieverBuilder {
    show_progress_bar: bool,
    pb_style: Option<ProgressStyle>,
    workers: usize,
}

impl Default for RetrieverBuilder {
    /// Creates a new Retriever builder.
    fn default() -> Self {
        Self {
            show_progress_bar: false,
            pb_style: Some(
                ProgressStyle::with_template(
                    "[{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}",
                )
                .expect("progress bar template should compile")
                .progress_chars("=>-"),
            ),
            workers: 10,
        }
    }
}

impl RetrieverBuilder {
    /// Creates a new Retriever builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets if progress bar will be shown.
    pub fn show_progress(mut self, show_progress_bar: bool) -> Self {
        self.show_progress_bar = show_progress_bar;
        self
    }

    /// Sets progress bar style.
    pub fn progress_style(mut self, pb_style: ProgressStyle) -> Self {
        self.pb_style = Some(pb_style);
        self
    }

    /// Sets the number of workers.
    pub fn workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    /// Creates a Retriever with the configured options.
    pub fn build(self) -> Retriever {
        Retriever {
            client: reqwest::Client::new(),
            job_semaphore: Semaphore::new(self.workers),
            mp: if self.show_progress_bar {
                Some(MultiProgress::new())
            } else {
                None
            },
            pb_style: self.pb_style,
        }
    }
}

/// Provies an easy interface for parallel downloads with limited workers and progress bar
///
/// # Examples
///
/// ```
/// use reqwest::Client;
/// use retriever::Retriever;
/// use tokio::fs::OpenOptions;
///
/// #[tokio::main]
/// async fn main() {
///     // create a retriever
///     let retriever = Retriever::with_progress_bar();
///
///     // open a file to write to
///     let file = OpenOptions::new()
///         .create(true)
///         .write(true)
///         .truncate(true)
///         .open(format!("index.html"))
///         .await
///         .expect("should return file");
///
///     // setup a request to retrieve the file
///     let req = Client::new().get("https://example.com").build().unwrap();
///
///     // download a file
///     let _  = retriever.download_file(req, file).await;
/// }
/// ```
pub struct Retriever {
    client: reqwest::Client,
    job_semaphore: Semaphore,
    mp: Option<MultiProgress>,
    pb_style: Option<ProgressStyle>,
}

impl Default for Retriever {
    /// Create a default retriever with 10 workers
    fn default() -> Self {
        Self {
            client: reqwest::Client::new(),
            job_semaphore: Semaphore::new(10),
            mp: None,
            pb_style: None,
        }
    }
}

impl Retriever {
    /// Same as default retriever but showing progress bar
    pub fn with_progress_bar() -> Self {
        Self {
            client: reqwest::Client::new(),
            job_semaphore: Semaphore::new(10),
            mp: Some(MultiProgress::new()),
            pb_style: None,
        }
    }

    /// Makes a request using a request and writes output into writer
    pub async fn download_file<W>(&self, request: Request, mut writer: W) -> Result<()>
    where
        W: AsyncWriteExt + Unpin + Send + Sync + 'static,
    {
        let _permit = self.job_semaphore.acquire().await?;

        let path = String::from(request.url().path());
        let mut resp = self.client.execute(request).await?;

        let mut pb: ProgressBar = ProgressBar::hidden();
        let content_len = resp.content_length();
        if let Some(total_size) = content_len {
            if let Some(m) = &self.mp {
                if let Some(pb_style) = &self.pb_style {
                    pb = m.add(
                        ProgressBar::new(total_size)
                            .with_style(pb_style.clone())
                            .with_message(path),
                    );
                }
            }
        }

        drop(_permit);

        let disk_write = tokio::spawn(async move {
            let mut written: u64 = 0;
            while let Some(chunk) = resp.chunk().await.unwrap() {
                writer.write_all(chunk.as_ref()).await.unwrap();
                writer.flush().await.unwrap();
                if let Some(_) = content_len {
                    written += chunk.len() as u64;
                    pb.set_position(written);
                }
            }

            pb.finish();
        });

        let _ = tokio::join!(disk_write);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Matcher;
    use reqwest::Client;
    use tokio::{fs::OpenOptions, io::AsyncReadExt};

    #[tokio::test]
    async fn download_single() {
        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("GET", Matcher::Regex(String::from("/\\d")))
            .with_status(200)
            .with_body("hello")
            .create();

        let retriever = RetrieverBuilder::new()
            .show_progress(false)
            .workers(1)
            .build();

        let req = Client::new()
            .get(format!("{}/1", server.url()))
            .build()
            .unwrap();

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open("/tmp/test")
            .await
            .unwrap();

        let _ = retriever.download_file(req, file).await;

        let mut file2 = OpenOptions::new()
            .read(true)
            .open("/tmp/test")
            .await
            .unwrap();

        let mut s = String::new();
        let _ = file2.read_to_string(&mut s).await;

        assert_eq!(s, "hello");

        mock.assert();
    }

    #[tokio::test]
    async fn download_multi() {
        use std::sync::Arc;
        use tokio::task::JoinSet;

        let mut server = mockito::Server::new_async().await;

        let mock = server
            .mock("GET", Matcher::Regex(String::from("/\\d")))
            .with_status(200)
            .with_body("hello")
            .expect(10)
            .create();

        let retriever = Arc::new(RetrieverBuilder::new().show_progress(true).build());

        let mut set = JoinSet::new();

        for i in 0..10 {
            let ret = Arc::clone(&retriever);
            let req = Client::new()
                .get(format!("{}/{}", server.url(), i))
                .build()
                .unwrap();

            let file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(format!("/tmp/test{}", i))
                .await
                .expect("file should be accessible");

            set.spawn(async move { ret.download_file(req, file).await });
        }

        while let Some(download_result) = set.join_next().await {
            assert!(!download_result.is_err());
        }

        mock.assert();
    }
}
