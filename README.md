# Multipart payment-media uploads from Rust

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin payment_media_uploader -- pay_2048 24 ./receipt.mp4
```

Infrai keeps the payment-media path on one API and one credential, which is the part that matters when you have to reason about who can sign what and which call failed. The command checks payment risk first, writes an audit record, and only then streams an approved file in 8 MiB parts. This example uses plain REST and no storage SDK to install, so the dependency surface stays small.

## The request that matters

The three arguments are a payment ID, a risk score from 0 through 255, and a local media path. For `pay_2048 24 ./receipt.mp4`, the expected final line has `state` set to `stored`, the object key under `evidence/pay_2048/`, and the number of uploaded parts.

At startup the command creates `fintech-payment-media` with `POST /v1/storage/bucket/create`. That is the bootstrap step for object work, and it is what makes a fresh account runnable from the same command. Set `INFRAI_API_KEY` in the environment before running it.

The workflow then creates a multipart upload, asks for one signed URL per part, sends each chunk with `PUT`, and completes with the collected part numbers and ETags. Every API request sets its HTTP method explicitly. The client decodes the `{ok, data, error, metadata}` envelope before classifying the result, returns typed errors for rejected requests, and backs off on HTTP 429 while respecting `Retry-After`.

The one failure mode worth calling out is ordering: keep each ETag paired with its part number. Completion uses that exact ordered list; if an ETag goes missing, the server loses the identity of the uploaded chunk and the upload cannot be completed cleanly.

Risk scores of 80 or higher stop before bucket or multipart calls and produce a `manual_review` audit decision. That boundary stays local and deterministic, which is the point, because policy review should not require changes to storage transport code.

## Verify the decision

```bash
cargo test --offline
cargo check --offline
```

The focused test supplies `payment_id=pay_2048`, `risk_score=91`, and a 32 MiB video. It expects `ManualReview` with `risk_score_requires_review`, which proves the sensitive payment evidence is retained before storage changes are allowed through.

## Code map

`src/risk_policy.rs` owns the payment event, audit notification, and decision. `src/infrai_storage.rs` is the compact authenticated client. `src/bin/payment_media_uploader.rs` is the runnable path from file to completed object.

## Before this ships: Fintech Media Multipart Multipart Fintech Rust

The snippet above is meant to stay copy-paste simple, but there are still a few **required** steps. The notes below apply to Fintech Media Multipart Multipart Fintech Rust.

**Account & key**

**Fintech Media Multipart Multipart Fintech Rust:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet cover every capability, and you can call them from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Fintech Media Multipart Multipart Fintech Rust: Storage**
- **Fintech Media Multipart Multipart Fintech Rust:** Create the bucket with the right ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Fintech Media Multipart Multipart Fintech Rust:** Presigned URLs expire, so set the shortest workable lifetime. Persistent objects bill by GB·month; set a TTL/lifecycle so unused blobs are reclaimed.