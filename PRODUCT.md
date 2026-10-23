# Product

<!-- impeccable:product-schema 1 -->

## Platform
web

## Stack
Rust Axum server serving one static HTML/CSS/JS page (no JS build). Runs as a Finder-launchable Mac app locally, and on AWS App Runner with S3 for the team. Chosen by the user during planning.

## Brand Commitments
Visual reference is the user's pixel-id-maker repo (AWS Student Builder Group pixel ID tools): ink-violet chrome, lavender accent, Silkscreen + Space Grotesk, purple pixel-fade. Not the IntPortal registrar theme.

## Users
A photographer/creative on a company creatives team who shoots events (RAW in Capture One), exports finished JPEG/PNG, and posts them to the team's social accounts. Teammates may share the same tool through one shared password.

## Product Purpose
Drop finished photos in, get files sized and encoded for Instagram and Facebook at the best quality those platforms keep, plus panorama splits for seamless carousel swipes. Success: upload the exported files as-is and see no extra softening, banding, or crop surprises.

## Positioning
Exact platform pixel targets with 4:4:4 JPEG and the source color profile kept, and a column splitter where the user sets column count and panel ratio and no source pixel is ever cropped.

## Operating Context
Used at a desk after a shoot, with batches of tens to hundreds of images. Downstream: manual upload in the Instagram/Facebook apps. Upstream: Capture One exports.

## Capabilities and Constraints
- Presets: IG square 1080x1080, portrait 1080x1350, landscape 1080x566, story 1080x1920; FB feed 2048 long side, story 1080x1920.
- Splitter: user sets columns and ratio; padding color fills leftover.
- Output is a zip download; past batches can be re-downloaded.
- No auto-posting. No HEIC/RAW input.

## Product Principles
- The photo is the subject; the tool stays out of its way.
- Never lose pixels silently: crops and pads are previewed before export.
- Fast batch over fine per-image fiddling.
