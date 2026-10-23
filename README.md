# Post Prep

Drop finished photos, get Instagram and Facebook files at the sizes the platforms keep (JPEG q95, 4:4:4 chroma, source color profile kept), plus carousel splits: you set the column count and panel ratio, the leftover gets padded, and no pixel is cropped.

Rust (Axum) server with one static page. Batches are stored as zips, in `./data` locally or in S3 when `S3_BUCKET` is set.

## Mac app (open from Finder)

```bash
./make-app.sh
```

This installs **Post Prep.app** in your home Applications folder (Finder, then Go, then Home, then Applications, or search it in Spotlight). Opening it starts the server on `127.0.0.1:4870`, local only with no password, and opens it in your browser. Batches are kept in `~/Library/Application Support/Post Prep`. Rerun the script after changing the code. The server keeps running in the background; to stop it:

```bash
pkill -f 'Post Prep.app/Contents/Resources/photo-post-prep'
```

## Run locally from source

```bash
cargo run --release
```

Open http://localhost:8080. Set `APP_PASSWORD` to turn on the login page.

## Test

```bash
cargo test
```

## Deploy to AWS (App Runner + S3)

Needs the AWS CLI v2 signed in and Docker running.

```bash
AWS_REGION=ap-southeast-1 ./deploy.sh
```

The first run creates a private bucket, an ECR repo, two IAM roles and the App Runner service, then asks for the shared password. Later runs push a new image and redeploy. The service is pinned to one instance because login sessions are held in memory.

| Env var | Meaning |
|---|---|
| `APP_PASSWORD` | Shared login password. Unset means no login. |
| `S3_BUCKET` | Store batches in S3. Unset means `./data`. |
| `DATA_DIR` | Local batch folder, default `data`. |
| `PORT` | Default 8080. |
| `HOST` | Bind address, default `0.0.0.0`. The Mac app uses `127.0.0.1`. |
