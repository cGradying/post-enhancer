# Post Prep

Photo export desk for social posts and photoshoots, by [cgradying](https://github.com/cGradying). Rust (Axum) server with one static page.

**Post Prep tab.** Instagram and Facebook files at the sizes the platforms keep, carousel splits (you set the columns and panel ratio; leftover space is padded, never cropped), and upscaled originals ×2/×4.

**Template tab.** Drop a photoshoot batch and every photo is placed on a branded frame: 3:4 by default, with 4:5, 1:1, 4:3, 3:2, 16:9 or a custom canvas. Each frame has margins, a border, rounded corners and a drop shadow. The background is a solid color, a blur of the photo, a pixel fade or an image. A title, credit line, logo and watermark (corner, center or tiled) go on top. Templates save in the browser and export to or import from `.json`.

**Shared.**
- **Formats:** JPEG, PNG, WebP, TIFF, BMP and GIF in. The Mac app also reads HEIC, AVIF and camera RAW through macOS.
- **Output:** JPEG (4:4:4, color profile kept) or PNG. Artist, Copyright and Software metadata are written to every file.
- **Upscaling:** Lanczos, or AI with Real-ESRGAN in the Mac app.
- **Processing:** auto enhance, batch rename (`{name} {event} {date} {n:03} {preset}`), live progress, and past batches you can download again.

## Mac app (open from Finder)

```bash
./make-app.sh
```

This installs **Post Prep.app** in your home Applications folder (Finder, then Go, then Home, then Applications, or search it in Spotlight). `make-app.sh` downloads Real-ESRGAN's official macOS build (checksum-pinned) into `vendor/` once and bundles it, which turns on AI upscaling. Opening the app starts the server on `127.0.0.1:4870`, local only with no password, and opens it in your browser. Batches are kept in `~/Library/Application Support/Post Prep`. Rerun the script after changing the code. The server keeps running in the background; to stop it:

```bash
pkill -f 'Post Prep.app/Contents/.*photo-post-prep'
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
| `MAX_UPLOAD_MB` | Upload limit per batch, default 1024. |
| `REALESRGAN_BIN` | Path to `realesrgan-ncnn-vulkan` to turn on AI upscaling outside the Mac app. |
| `HOST` | Bind address, default `0.0.0.0`. The Mac app uses `127.0.0.1`. |
