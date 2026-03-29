use clap::{Parser, Subcommand};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::error::Error;

#[derive(Parser)]
#[command(author, version, about = "Google Photos CLI for MCP", long_about = None)]
struct Cli {
    #[arg(short, long, env = "GOOGLE_ACCESS_TOKEN", help = "OAuth Access Token")]
    token: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List albums
    ListAlbums {
        #[arg(long, default_value_t = 50)]
        page_size: u32,
    },
    /// List media items
    ListMedia {
        #[arg(long)]
        album_id: Option<String>,
        #[arg(long, default_value_t = 50)]
        page_size: u32,
    },
    /// Get a media item
    GetMedia {
        #[arg(long)]
        media_item_id: String,
    },
}

#[derive(Serialize, Deserialize, Debug)]
struct Album {
    id: String,
    title: String,
    #[serde(rename = "productUrl")]
    product_url: String,
    #[serde(rename = "mediaItemsCount")]
    media_items_count: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
struct MediaItem {
    id: String,
    description: Option<String>,
    #[serde(rename = "productUrl")]
    product_url: String,
    #[serde(rename = "baseUrl")]
    base_url: String,
    #[serde(rename = "mimeType")]
    mime_type: String,
    #[serde(rename = "filename")]
    filename: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let client = reqwest::Client::new();

    match &cli.command {
        Commands::ListAlbums { page_size } => {
            let url = format!("https://photoslibrary.googleapis.com/v1/albums?pageSize={}", page_size);
            let res = client
                .get(&url)
                .header(AUTHORIZATION, format!("Bearer {}", cli.token))
                .send()
                .await?;

            if !res.status().is_success() {
                let err_text = res.text().await?;
                eprintln!("API Error: {}", err_text);
                std::process::exit(1);
            }

            let text = res.text().await?;
            println!("{}", text);
        }
        Commands::ListMedia { album_id, page_size } => {
            let url = "https://photoslibrary.googleapis.com/v1/mediaItems:search".to_string();
            let mut body = serde_json::Map::new();
            body.insert("pageSize".to_string(), serde_json::json!(page_size));
            if let Some(id) = album_id {
                body.insert("albumId".to_string(), serde_json::json!(id));
            }

            let res = client
                .post(&url)
                .header(AUTHORIZATION, format!("Bearer {}", cli.token))
                .header(CONTENT_TYPE, "application/json")
                .json(&body)
                .send()
                .await?;

            if !res.status().is_success() {
                let err_text = res.text().await?;
                eprintln!("API Error: {}", err_text);
                std::process::exit(1);
            }

            let text = res.text().await?;
            println!("{}", text);
        }
        Commands::GetMedia { media_item_id } => {
            let url = format!("https://photoslibrary.googleapis.com/v1/mediaItems/{}", media_item_id);
            let res = client
                .get(&url)
                .header(AUTHORIZATION, format!("Bearer {}", cli.token))
                .send()
                .await?;

            if !res.status().is_success() {
                let err_text = res.text().await?;
                eprintln!("API Error: {}", err_text);
                std::process::exit(1);
            }

            let text = res.text().await?;
            println!("{}", text);
        }
    }

    Ok(())
}
