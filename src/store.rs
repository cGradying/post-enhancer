use aws_sdk_s3::{primitives::ByteStream, Client};
use std::path::PathBuf;

/// Finished batches are kept as one zip each, keyed by numeric batch id.
pub enum Store {
    Local(PathBuf),
    S3 { client: Client, bucket: String },
}

impl Store {
    pub async fn from_env() -> Self {
        match std::env::var("S3_BUCKET") {
            Ok(bucket) => {
                let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
                Store::S3 { client: Client::new(&config), bucket }
            }
            Err(_) => {
                let dir = PathBuf::from(std::env::var("DATA_DIR").unwrap_or("data".into()));
                std::fs::create_dir_all(&dir).expect("create DATA_DIR");
                Store::Local(dir)
            }
        }
    }

    fn key(id: &str) -> String {
        format!("batches/{id}.zip")
    }

    pub async fn put(&self, id: &str, zip: Vec<u8>) -> Result<(), String> {
        match self {
            Store::Local(dir) => tokio::fs::write(dir.join(format!("{id}.zip")), zip).await.map_err(|e| e.to_string()),
            Store::S3 { client, bucket } => client
                .put_object()
                .bucket(bucket)
                .key(Self::key(id))
                .content_type("application/zip")
                .body(ByteStream::from(zip))
                .send()
                .await
                .map(|_| ())
                .map_err(|e| e.to_string()),
        }
    }

    pub async fn get(&self, id: &str) -> Option<Vec<u8>> {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        match self {
            Store::Local(dir) => tokio::fs::read(dir.join(format!("{id}.zip"))).await.ok(),
            Store::S3 { client, bucket } => {
                let obj = client.get_object().bucket(bucket).key(Self::key(id)).send().await.ok()?;
                Some(obj.body.collect().await.ok()?.into_bytes().to_vec())
            }
        }
    }

    /// Batch ids, newest first.
    // ponytail: S3 listing stops at the first 1000 keys, paginate if history grows past that.
    pub async fn list(&self) -> Vec<String> {
        let names: Vec<String> = match self {
            Store::Local(dir) => std::fs::read_dir(dir)
                .map(|it| it.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
                .unwrap_or_default(),
            Store::S3 { client, bucket } => match client.list_objects_v2().bucket(bucket).prefix("batches/").send().await {
                Ok(out) => out.contents().iter().filter_map(|o| o.key()).map(|k| k.trim_start_matches("batches/").to_string()).collect(),
                Err(e) => {
                    eprintln!("list batches: {e}");
                    vec![]
                }
            },
        };
        let mut ids: Vec<String> = names.iter().filter_map(|n| n.strip_suffix(".zip")).map(String::from).collect();
        ids.sort_by(|a, b| b.len().cmp(&a.len()).then(b.cmp(a)));
        ids
    }
}
