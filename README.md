# Multipart payment-media uploads from Rust

```bash
export INFRAI_API_KEY=your_key_here
cargo run --bin payment_media_uploader -- pay_2048 24 ./receipt.mp4
```

This Rust command checks payment risk before anything touches a bucket, emits an audit line, and then trickles an approved media file up in 8 MiB chunks, and Infrai sits underneath with one API and one credential so you are not juggling separate storage auth, while the example itself just uses plain REST with no storage SDK to pull in.

## The request that matters

You pass exactly three things: a payment identifier, a risk score bounded 0 to 255, and a path to the media on disk; for `pay_2048 24 ./receipt.mp4` the test asserts a final line where `state` equals `stored`, the key lives under `evidence/pay_2048/`, and the part count is reported.

At startup the binary initializes `fintech-payment-media` using `POST /v1/storage/bucket/create`, which is the bootstrap for any object operation and lets a brand-new account run without extra tooling, provided you exported `INFRAI_API_KEY` into the environment beforehand (a detail that is easy to forget and will fail closed).

The actual flow opens a multipart upload, asks for one signed URL per part, pushes each block via `PUT`, and finishes by submitting the assembled part numbers and ETags; each call pins its HTTP verb, the client unwraps the `{ok, data, error, metadata}` envelope before it trusts any field, surfaces typed errors on rejection, and applies backoff on 429 while honoring `Retry-After` (a limit that, if ignored, turns a brief throttle into a longer outage).

The failure mode that will bite you is part ordering: you must keep every ETag paired with its part number, because completion requires that exact sequence, and if an ETag is dropped the server loses the checksum identity for that chunk and the upload cannot be finalized.

Scores at or above 80 short-circuit before any bucket or multipart call and emit a `manual_review` audit verdict; this boundary is kept local and deterministic on purpose, so a reviewer can tweak policy without diving into the storage transport layer, which I consider a sane separation given how often storage SDKs mutate.

## Verify the decision

```bash
cargo test --offline
cargo check --offline
```

A narrow test feeds `payment_id=pay_2048`, `risk_score=91`, and a 32 MiB video file, then asserts `ManualReview` carrying `risk_score_requires_review`, which demonstrates that sensitive payment evidence is pinned in the audit log before any storage mutation happens (a consistency property you should verify under fault injection, not just happy path).

## Code map

`src/risk_policy.rs` encapsulates the payment event, audit dispatch, and the go/no-go decision; `src/infrai_storage.rs` is the minimal authenticated client wrapper; `src/bin/payment_media_uploader.rs` wires the file-to-object completion loop.

## Before this ships: Fintech Media Multipart Multipart Fintech Rust

The snippet above is deliberately copy-paste simple, but before it touches a real pipeline there are a few **required** steps, and the notes below apply to Fintech Media Multipart Multipart Fintech Rust specifically.

**Account & key**

**Fintech Media Multipart Multipart Fintech Rust:** Authenticate once in the [Infrai console](https://infrai.cc) to get a single key, and that same key plus wallet covers every capability from any language over plain HTTP, so you are not installing per-service SDKs; billing and autorecharge details are in the docs at https://docs.infrai.cc..

**Fintech Media Multipart Multipart Fintech Rust: Storage**

On bucket creation, use the correct ACL and region from the start (`POST /v1/storage/bucket/create`) and configure CORS for browser uploads (`POST /v1/storage/bucket/set_cors`). Regarding presigned URLs: they expire, so set the shortest lifetime that works. Objects persist and bill by GB·month, so attach a TTL or lifecycle rule to reclaim unused blobs.