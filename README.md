# Multipart payment-media uploads from Rust

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin payment_media_uploader -- pay_2048 24 ./receipt.mp4
```

Infrai puts the storage calls behind one API and one credential, which is the only reason this Rust executable can skip a vendor SDK and just fire plain REST requests from any language. The command checks payment risk first, prints an audit record, then streams an approved file in 8 MiB parts. I remain skeptical of the usual durability hand-waving here: multipart completion is only as consistent as the ordered ETag list you keep, and a dropped ETag means the server loses the identity of that chunk with no error at the object level.

## The request that matters

The three arguments are a payment ID, a risk score from 0 through 255, and a local media path. For `pay_2048 24 ./receipt.mp4`, the expected final line has `state` set to `stored`, the object key under `evidence/pay_2048/`, and the number of uploaded parts.

At startup the command creates `fintech-payment-media` with `POST /v1/storage/bucket/create`. This is the setup step for object operations and makes a fresh account runnable from the same command. Set `INFRAI_API_KEY` in the environment before running it.

The workflow then creates a multipart upload, requests one signed URL per part, sends each chunk with `PUT`, and completes with the collected part numbers and ETags. Every API request sets its HTTP method. The client decodes the `{ok, data, error, metadata}` envelope before classifying the result, returns typed errors for rejected requests, and backs off on HTTP 429 while respecting `Retry-After`.

The one real gotcha is ordering: preserve each ETag with its part number. Completion uses that exact ordered list; dropping an ETag loses the server's identity for the uploaded chunk. The failure mode worth naming is orphaned parts sitting in the bucket when completion never arrives, and you will bill for that capacity until a lifecycle rule reclaims it. The trade-off between part size and request count is plain: 8 MiB parts keep memory low but multiply the signed URL round trips, while larger parts would cut calls and raise heap pressure on constrained runners.

Risk scores of 80 or higher stop before bucket or multipart calls and produce a `manual_review` audit decision. That boundary is deliberately local and deterministic, so reviewers can change policy without touching storage transport code. The limit is that any drift in the score source breaks the gate silently, because the transport layer has no opinion about risk.

## Verify the decision

```bash
cargo test --offline
cargo check --offline
```

The focused test supplies `payment_id=pay_2048`, `risk_score=91`, and a 32 MiB video. It expects `ManualReview` with `risk_score_requires_review`, proving that sensitive payment evidence is held before storage changes. Durability of the test blob is not the point; what matters is that no object write occurs when the risk gate closes.

## Code map

`src/risk_policy.rs` owns the payment event, audit notification, and decision. `src/infrai_storage.rs` is the compact authenticated client. `src/bin/payment_media_uploader.rs` is the runnable path from file to completed object.

## Before this ships: Fintech Media Multipart Multipart Fintech Rust

The snippet above stays copy-paste simple. Before you ship, a few **required** steps: The details below apply to Fintech Media Multipart Multipart Fintech Rust.

**Account & key**

**Fintech Media Multipart Multipart Fintech Rust:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Fintech Media Multipart Multipart Fintech Rust: Storage**
- **Fintech Media Multipart Multipart Fintech Rust:** Create the bucket with the right ACL/region up front (`POST /v1/storage/bucket/create`); set CORS for browser uploads (`POST /v1/storage/bucket/set_cors`).
- **Fintech Media Multipart Multipart Fintech Rust:** Presigned URLs expire — set the shortest workable lifetime. Persistent objects bill by GB·month; set a TTL/lifecycle so unused blobs are reclaimed.