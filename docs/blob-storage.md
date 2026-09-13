# Encrypted blob storage

Links stores media as client-encrypted opaque blobs. The storage service never
receives plaintext media, private media keys, BlurHash values, or message
content. The shared server boundary is `S3CompatibleBlobStore` in
`links-server-store::blob`.

## Object contract

The authenticated attachment service accepts a UUID attachment ID and
ciphertext only. It writes the object at:

`links/v1/blobs/<canonical-attachment-uuid>`

Each upload is limited to 32 MiB and uses:

- content type `application/octet-stream`
- `Cache-Control: public, max-age=2592000, immutable`
- SHA-256 metadata and an exact byte size
- conditional creation equivalent to `If-None-Match: *`

Retries with the same ID are accepted only when size, digest, and object
metadata match. A different body returns Conflict. Downloads verify the
ciphertext against the size and SHA-256 stored in the private
`MediaMetadata`; mismatches fail closed. Deletes are idempotent and
run from the retention worker after the private message retention window.

## Cloudflare R2

1. Create a private R2 bucket for encrypted blobs.
2. Give only the attachment service an R2 S3 access key. Do not expose the
   secret or bucket endpoint to clients.
3. Put a Worker or authenticated attachment endpoint in front of the bucket.
   It validates a short-lived signed path for the UUID, then reads the R2
   object. Never authorize by a user-controlled filename or MIME type.
4. Add a Cache Rule for `/v1/blobs/*` that caches only successful
   GET/HEAD responses, ignores tracking query parameters, and honors the
   immutable 30-day object policy. Do not enable image resizing, content
   transformation, or public bucket listing.
5. Add an R2 lifecycle rule to delete objects after the message retention
   window. Keep access logs free of full signed URLs and request bodies.

## Amazon S3 + CloudFront

1. Create a private S3 bucket with public access blocked, versioning disabled
   unless a deletion audit requires it, and a lifecycle expiration at the
   attachment retention limit.
2. Create a CloudFront Origin Access Control using SigV4. The bucket policy
   grants read access only to that distribution and write/delete access only to
   the attachment service role.
3. Add a `/v1/blobs/*` behavior: HTTPS only, GET/HEAD only, no
   origin redirects, no request-body forwarding, and a cache policy with no
   PII-bearing query strings. Enforce short-lived CloudFront signed URLs at
   the edge.
4. Set the default, minimum, and maximum cache TTL to the 30-day retention
   window. The cached value is still ciphertext, so edge nodes cannot render
   or inspect the media.
5. Configure access logs and metrics to record status, size, and latency, but
   not bearer tokens, signed URL query values, message IDs, or plaintext.

Cloudflare R2 and Amazon S3 are interchangeable behind the same
`S3CompatibleObjectClient` interface. Provider clients must use TLS,
bounded timeouts, conditional writes, server-side encryption at rest, and
metrics that omit object bodies and cryptographic material.
