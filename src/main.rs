use anyhow::Context;
use futures::stream::StreamExt;
use selene;
use selene::schoolism::client;
use std::{
    io::{BufReader, Seek, SeekFrom},
    sync::Arc,
};

use clap::Clap;
use std::fs::File;
use tokio::fs::File as TokioFile;
use tokio::io::copy;

#[derive(Clap)]
#[clap(version = "1.0", author = "dtn", about = "selene")]
struct Opts {
    #[clap(short, long)]
    username: String,
    #[clap(short, long)]
    password: String,
    #[clap(long, about = "Index of lesson to download")]
    lesson: usize,
    #[clap(long, about = "Index of part to download")]
    part: usize,
    #[clap(long, default_value = "4", about = "Parallel downloads allowed")]
    parallel: usize,
    #[clap(long, about = "Request high quality video")]
    hq: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let opts: Opts = Opts::parse();

    // TODO get lesson name from scraping
    let file_out_name = {
        let quality = if opts.hq { "hq" } else { "nq" };
        format!(
            "selene_lesson_{}_part_{}_{}.mp4",
            opts.lesson, opts.part, quality
        )
    };

    println!("saving file to [{}]", file_out_name);

    // establish connection
    let client = client::ClientInit::new(&opts.username, &opts.password)?
        .connect()
        .await?;

    println!("connected to schoolism");

    // get playlist for chosen lesson and part, if it doesn't exist, bail
    let list = client.get_playlist(opts.lesson, opts.part, opts.hq).await?;
    let cipher = Arc::new(selene::decryption::Cipher::from_list(&list));

    println!("retrieved playlist details");

    let download_client = Arc::new(reqwest::Client::new());

    println!(
        "downloading [{}] parts with [{}] threads",
        list.files.len(),
        opts.parallel
    );

    // parse playlist into file parts
    // TODO error handling here
    let tasks_iter = list.files.into_iter().map(|url| {
        let cipher = cipher.clone();
        let download_client = download_client.clone();

        async move {
            let start = std::time::Instant::now();

            // prepare fs
            let f = tempfile::tempfile().unwrap();
            let mut f = TokioFile::from_std(f);

            // download
            let resp = download_client.get(&url).send().await.unwrap();
            let mut bytes = resp.bytes().await.unwrap().to_vec();

            // decrypt
            let bytes = cipher.decrypt(&mut bytes).unwrap();
            let b = copy(&mut &bytes[..], &mut f).await.unwrap();

            let duration = start.elapsed();
            println!(
                "completed download and decryption of [{}], [{}] bytes, took [{:.2?}]",
                url, b, duration
            );

            f
        }
    });

    let file_handles = futures::stream::iter(tasks_iter)
        .buffered(opts.parallel)
        .collect::<Vec<TokioFile>>()
        .await;

    let out_file = File::create(&file_out_name).context("could not create final output file")?;

    println!("download complete, merging files");
    for i in file_handles {
        let mut i = i.into_std().await;
        i.seek(SeekFrom::Start(0))
            .context("could not seek tempfile")?;

        let mut r = BufReader::new(i);
        let _b =
            std::io::copy(&mut r, &mut &out_file).context("failed to write part to end file")?;
    }

    println!("all parts merged [{}]", file_out_name);

    Ok(())
}
