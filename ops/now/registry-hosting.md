---
status: brainstorm
---

# Registry Hosting Options

**Status**: Brainstorm
**Related**: [Plugin Registry](registry.md)

---

## Service Profile

From the [registry spec](registry.md), the service is:

- **A Rust binary** serving a `plugins.json` file from memory
- **Read-heavy, write-rare** — CLI clients search/read; scraper updates occasionally
- **Stateless** — loads JSON at startup, no database
- **Low traffic** — CLI-only clients, no web UI in v1
- **Tiny compute** — in-memory linear scan over hundreds to low-thousands of entries
- **Two APIs** — public read-only REST + token-authenticated admin (scraper ingest)

This is about as simple as a hosted service gets: a single binary, no database, no persistent connections, minimal CPU/memory. The main requirements are HTTPS, reasonable uptime, and a stable URL the CLI can hardcode.

---

## Options

### 1. Fly.io

**What**: Deploy a Docker container (or Fly-native Rust binary) to Fly's edge infrastructure. Machines run on-demand or always-on.

**Why it fits**:
- Dead-simple deploy for a single Rust binary (Dockerfile or `fly launch`)
- Machines can scale to zero and wake on request (pay nothing when idle)
- Built-in TLS, custom domains, health checks
- Persistent volumes available if we want to store `plugins.json` on disk (though we could also fetch from object storage on startup)
- Multi-region easy to add later if latency matters

**Costs**: ~$2/mo for a shared 256MB VM running continuously. Potentially $0 with scale-to-zero if traffic is bursty. +$2/mo for dedicated IPv4 (optional). Free tier includes 3 shared VMs and 160GB bandwidth.

**Tradeoffs**:
- (+) Excellent DX — `fly deploy` and done
- (+) Scale-to-zero means near-zero cost at low traffic
- (+) Easy to add regions later
- (-) Cold starts if scaled to zero (Rust starts fast, so likely <500ms)
- (-) Usage-based billing can surprise at scale (but we're nowhere near that)
- (-) Vendor lock-in is minimal — it's just a container

**Verdict**: Strong default choice. Lowest friction, lowest cost at our scale.

---

### 2. Cloudflare Workers (WASM)

**What**: Compile the registry service to WASM via `workers-rs`, deploy to Cloudflare's edge network. Data stored in KV or R2.

**Why it fits**:
- Runs at the edge globally — lowest possible latency for CLI clients worldwide
- Free tier is generous (100k requests/day)
- KV is perfect for storing the plugin index (read-heavy, write-rare)
- No cold starts — Workers are always warm

**Costs**: Free tier covers 100k requests/day. Paid plan $5/mo for 10M requests. KV storage essentially free at our scale.

**Tradeoffs**:
- (+) Global edge deployment — latency is as good as it gets
- (+) Truly scale-to-zero with no cold starts
- (+) KV is a natural fit for our read-heavy, write-rare data pattern
- (-) Must compile Rust to WASM — some crates don't compile cleanly
- (-) 128MB memory limit per Worker
- (-) WASM bundle size limits — need to keep the binary lean
- (-) Can't do filesystem operations; must use KV/R2 for `plugins.json`
- (-) Architectural constraints: request/response model, no long-running processes
- (-) Debugging is harder (WASM stack traces, different runtime behavior)

**Verdict**: Compelling for latency and free tier, but the WASM compilation constraints add friction. Worth prototyping to see if the Rust code compiles cleanly.

---

### 3. Google Cloud Run

**What**: Deploy a container that scales to zero. Google manages everything.

**Why it fits**:
- True scale-to-zero — pay nothing when no requests
- Generous free tier: 2M requests/mo, 180k vCPU-seconds, 360k GiB-seconds
- Container-based — same Dockerfile works anywhere
- Built-in TLS, custom domains, health checks

**Costs**: Likely $0/mo within free tier for our traffic levels. Even a constantly-running minimal instance is ~$3-5/mo. New customers get $300 free credits for 90 days.

**Tradeoffs**:
- (+) Free tier is enormous — we'd probably never leave it at v1 traffic
- (+) Google's infra reliability
- (+) Easy to add Cloud Storage for `plugins.json` persistence
- (+) Good monitoring/logging out of the box
- (-) Cold starts when scaled to zero (Rust mitigates this — likely <1s)
- (-) GCP console/IAM complexity is overkill for one service
- (-) Slight vendor lock-in via GCP-specific logging, metrics, etc.

**Verdict**: Excellent free tier makes this very attractive for a service that might have minimal traffic for a while. Good "set it and forget it" option.

---

### 4. AWS Lambda + API Gateway (Rust)

**What**: Deploy the registry as a Lambda function using the Rust runtime. API Gateway handles HTTP routing.

**Why it fits**:
- True scale-to-zero
- Free tier: 1M requests/mo and 400k GB-seconds
- 1ms billing granularity (efficient for fast responses)
- Our service is essentially request/response — fits Lambda model

**Costs**: Likely $0/mo within free tier. At moderate traffic (~100k req/mo), still <$1/mo.

**Tradeoffs**:
- (+) AWS free tier is generous for Lambda
- (+) Rust cold starts are fast (~100-200ms)
- (+) No server management at all
- (+) 1ms billing granularity is ideal for sub-millisecond in-memory searches
- (-) Must restructure code for Lambda's handler model (not a standard HTTP server)
- (-) API Gateway adds latency (~10-30ms) and complexity
- (-) Loading `plugins.json` on every cold start (can cache in /tmp across warm invocations)
- (-) API Gateway pricing adds up separately ($3.50/M requests)
- (-) AWS IAM/config complexity is significant for a simple service
- (-) Lambda URLs (function URLs) could skip API Gateway but lose some features

**Verdict**: Free and scalable, but the Lambda model requires reshaping the code. Better fit if we were starting from scratch with Lambda in mind.

---

### 5. AWS ECS Fargate / EC2

**What**: Run the container on Fargate (serverless containers) or a small EC2 instance.

**Why it fits**:
- Standard container deployment, no code changes
- Full control over runtime, networking, storage
- EC2 free tier: 1 year of t3.micro

**Costs**: Fargate ~$10-15/mo minimum (can't scale to zero). EC2 t3.micro ~$7-8/mo (or free for first year). EC2 t4g.nano ~$3/mo.

**Tradeoffs**:
- (+) Standard deployment model, portable
- (+) Full control over everything
- (+) EC2 free tier for first year
- (-) Always-on cost even with zero traffic
- (-) Fargate can't scale to zero
- (-) EC2 means managing the instance (updates, security patches)
- (-) Overkill infrastructure complexity for a JSON-serving binary

**Verdict**: The heavy option. Makes sense if we're already invested in AWS infrastructure, otherwise adds unnecessary complexity and cost.

---

### 6. Hetzner Cloud VPS

**What**: Rent a small VPS, deploy the binary directly or via Docker.

**Why it fits**:
- Cheapest always-on option
- Full root access, run whatever you want
- European data centers (good for GDPR if relevant)

**Costs**: CX22 at ~€3.79/mo (2 vCPU, 4GB RAM, 40GB disk). Rising to ~€4-5/mo after April 2026 price increase. Traffic, IPv4/IPv6, DDoS protection included.

**Tradeoffs**:
- (+) Cheapest always-on hosting
- (+) Massive overkill specs for our needs (2 vCPU, 4GB for a <10MB service)
- (+) Full control, no vendor abstractions
- (+) All-inclusive pricing — no surprise bandwidth/IP bills
- (-) Must manage the server: updates, security, monitoring, TLS certs (Let's Encrypt)
- (-) No scale-to-zero — paying even with zero traffic
- (-) Single point of failure unless we set up redundancy ourselves
- (-) Must handle deploys ourselves (systemd service, Docker, etc.)

**Verdict**: Best bang-for-buck if we're comfortable with light ops work. Good choice if we want a general-purpose box that can also run the scraper, host other tools, etc.

---

### 7. Railway

**What**: Git-push-to-deploy platform. Auto-detects stack, builds, and deploys.

**Why it fits**:
- Zero-config deploys for Rust/Docker
- Built-in databases if we ever need one
- Simple DX similar to Fly.io

**Costs**: Hobby plan $5/mo with $5 credits (covers most small projects). Usage-based beyond credits. No permanent free tier.

**Tradeoffs**:
- (+) Extremely simple DX — push to GitHub, app deploys
- (+) Usage-based within plan means light services cost nearly nothing beyond base fee
- (+) Good for teams — easy to share and manage
- (-) $5/mo minimum even if the service has zero traffic
- (-) No scale-to-zero
- (-) Less mature than Fly.io for container workloads
- (-) No background worker support if the scraper needs to run alongside

**Verdict**: Fine option, slightly worse value than Fly.io for our use case due to minimum monthly cost and no scale-to-zero.

---

### 8. Shuttle.dev (Rust-native)

**What**: Rust-native hosting platform. Add annotations to your Rust code, deploy with `cargo shuttle deploy`.

**Why it fits**:
- Built specifically for Rust — first-class support
- Zero infrastructure config — just annotate your main function
- Free community tier for hobby projects

**Costs**: Free community tier for side projects. Pro at $20/mo + usage for production.

**Tradeoffs**:
- (+) Best DX for Rust — literally `cargo shuttle deploy`
- (+) Free tier for experiments/early stage
- (+) Rust-native means no Docker, no WASM, no Lambda shims
- (-) Requires Shuttle-specific annotations in code (vendor lock-in at code level)
- (-) Relatively young platform — less battle-tested than AWS/GCP/Fly
- (-) $20/mo jump from free to pro is steep for a tiny service
- (-) Limited control over infrastructure details
- (-) Company raised $6M seed in Oct 2025 — still early stage

**Verdict**: Interesting for prototyping and the Rust DX story, but the code-level vendor lock-in and platform maturity concerns make it risky for a service we want to run reliably long-term.

---

### 9. DigitalOcean App Platform / Droplet

**What**: App Platform is a PaaS (similar to Railway/Fly). Droplets are VPS (similar to Hetzner).

**Why it fits**:
- Well-known, reliable, good docs
- App Platform handles TLS, deploys, health checks
- Droplets are simple and affordable

**Costs**: App Platform basic starts at $5/mo. Droplets from $4/mo (1 vCPU, 512MB). $200 free credit for new accounts (60 days).

**Tradeoffs**:
- (+) Mature platform, good reliability track record
- (+) App Platform is simple PaaS — push code, it deploys
- (+) Droplets are cheap and straightforward
- (-) App Platform doesn't scale to zero
- (-) No particular advantage over Fly.io or Cloud Run for this use case
- (-) Droplets require same ops work as Hetzner

**Verdict**: Solid but unremarkable. Pick this if there's an existing DO relationship or preference.

---

## Comparison Matrix

| Option | Monthly Cost | Scale to Zero | DX | Ops Burden | Vendor Lock-in | Cold Start |
|---|---|---|---|---|---|---|
| **Fly.io** | ~$0-4 | Yes | Excellent | Minimal | Low (container) | ~200-500ms |
| **CF Workers** | ~$0-5 | Yes | Good | Minimal | Medium (WASM) | None |
| **Cloud Run** | ~$0 | Yes | Good | Minimal | Low (container) | ~500-1000ms |
| **Lambda** | ~$0 | Yes | Moderate | Low | Medium (handler model) | ~100-200ms |
| **ECS/EC2** | ~$7-15 | No | Moderate | Medium | Low | N/A |
| **Hetzner** | ~€4-5 | No | Manual | High | None | N/A |
| **Railway** | ~$5 | No | Excellent | Minimal | Low | N/A |
| **Shuttle** | ~$0-20 | Unclear | Best (Rust) | Minimal | High (code annotations) | Unclear |
| **DigitalOcean** | ~$4-5 | No | Good | Low-Medium | Low | N/A |

---

## Recommendations

### For v1 (get it running, minimal traffic)

**Primary: Fly.io** or **Google Cloud Run**

Both offer scale-to-zero (pay nothing when idle), minimal ops, and container-based deployment that's portable. Fly.io has slightly better DX; Cloud Run has a more generous free tier.

The service is a stateless Rust binary that loads JSON into memory. Either platform deploys this in minutes with a Dockerfile.

### If we want to minimize cost indefinitely

**Google Cloud Run** — the free tier (2M requests/mo) will likely cover our needs for a long time, possibly forever at v1 traffic levels.

### If we want the best latency globally

**Cloudflare Workers** — edge deployment means sub-10ms responses worldwide. But only if the Rust code compiles cleanly to WASM.

### If we want a general-purpose server

**Hetzner** — at ~€4/mo we get a box that can run the registry, the scraper on a cron, and anything else. Requires ops comfort.

### Not recommended for this service

- **ECS/Fargate** — too much infrastructure complexity for a JSON-serving binary
- **Lambda** — requires reshaping the code for no clear benefit over Cloud Run
- **Shuttle** — code-level lock-in for a young platform is risky

---

## Next Steps

1. **Pick primary + fallback** — Recommend choosing one to deploy on, with a Dockerfile that makes migration trivial
2. **Domain** — Decide on the registry URL (e.g., `registry.aiki.dev`, `plugins.aiki.dev`)
3. **`plugins.json` storage** — Where does the scraper write to and the service read from? Options: baked into container image, object storage (S3/R2/GCS), persistent volume
4. **CI/CD** — GitHub Actions to build and deploy on push to main
5. **Monitoring** — At minimum: uptime check, error alerting
