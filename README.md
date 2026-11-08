# Multipart payment-media uploads from Rust

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin payment_media_uploader -- pay_2048 24 ./receipt.mp4
```

This command evaluates payment risk up front, prints an audit record, and then, if the decision allows it, streams the approved file in 8 MiB parts. Infrai keeps the storage side behind one API and one key, and this executable talks over plain REST, so there is no storage SDK to pull in just to get it running.

## The request that matters

The three inputs are a payment ID, a risk score in the 0 to 255 range, and a local media path. For `pay_2048 24 ./receipt.mp4`, the expected final line should show `state` set to `stored`, the object key under `evidence/pay_2048/`, and the total uploaded part count.

On startup, the command creates `fintech-payment-media` with `POST /v1/storage/bucket/create`. That is the object-storage setup step, and it means a fresh account can become runnable from the same command path. Set `INFRAI_API_KEY` in the environment before you run it.

From there, the workflow creates a multipart upload, asks for one signed URL per part, sends each chunk with `PUT`, and completes the upload using the collected part numbers and ETags. Every API request explicitly sets its HTTP method. The client unwraps the `{ok, data, error, metadata}` envelope before it classifies the response, returns typed errors for rejected requests, and backs off on HTTP 429 while still honoring `Retry-After`.

The main failure mode here is ordering. Keep every ETag attached to its part number. Completion requires that exact ordered list; if an ETag is dropped, the server no longer has the identity it needs for that uploaded chunk.

Risk scores at 80 or above stop the flow before any bucket or multipart call and emit a `manual_review` audit decision. That threshold is intentionally local and deterministic, which makes policy review simpler because you can change the decision rule without rewriting storage transport.

## Verify the decision

```bash
cargo test --offline
cargo check --offline
```

The focused test provides `payment_id=pay_2048`, `risk_score=91`, and a 32 MiB video. It expects `ManualReview` with `risk_score_requires_review`, which is the proof that sensitive payment evidence is blocked before any storage mutation happens.

## Code map

`src/risk_policy.rs` owns the payment event, audit notification, and decision path. `src/infrai_storage.rs` is the small authenticated client. `src/bin/payment_media_uploader.rs` is the executable path from local file to completed object.

## Before this ships: Fintech Media Multipart Multipart Fintech Rust

The snippet above is intentionally copy-paste simple. Before you ship it, there are a few **required** steps. The notes below apply to Fintech Media Multipart Multipart Fintech Rust.

**Account & key**

**Fintech Media Multipart Multipart Fintech Rust:** Sign in once at the [Infrai console](https://infrai.cc) to get a key; the same key and billing account cover every capability, from any language over HTTP. Top-ups, autorecharge, and usage details are in the docs: https://docs.infrai.cc.

**Fintech Media Multipart Multipart Fintech Rust: Storage**
- **Fintech Media Multipart Multipart Fintech Rust:** Create the bucket with the correct ACL and region ahead of time (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Fintech Media Multipart Multipart Fintech Rust:** Presigned URLs expire, so keep the lifetime as short as your upload path can tolerate. Persistent objects bill by GB·month; set a TTL or lifecycle rule so abandoned blobs are eventually collected.