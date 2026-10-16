# LavaGuard

LavaGuard started as an evolution of ideas from `LavaTools`. Initially, LavaTools was built as an all-in-one utility suite. But looking closely at the Lavalink ecosystem, the biggest pain points always came down to two things: YouTube's cipher throttling (`n-sig`) and BotGuard integrity verification.

Tools like `yt-cipher` (Deno) and `webpo-generator` (Node) each solve a slice of the problem, but they require heavy runtimes, separate services, and high memory footprints. 

That sparked a question: **What if we combine both into a single, low-level binary written in Rust?** 

No bloat, no multi-hundred megabyte browser instances, sub-millisecond execution, and rock-solid memory efficiency. That's why LavaGuard was created.

---

## What's Inside

- **Hybrid Solver Engine:**
  - **Fast-path (EJS AST):** Extracts and solves signature deciphering and n-sig challenges in `< 0.2ms`. Built-in algebraic XOR cancellation ($A \oplus A = 0$) for tricky TCL variants (`IAS_TCL`, `TV_TCL`, `player_ias_tcl`).
  - **Fallback-path (Eval Hook):** If YouTube ever rolls out a player script that breaks the AST patterns, LavaGuard automatically hooks `__solve_url` and evaluates it in a sandboxed QuickJS context with lightweight browser stubs.
- **Embedded JavaScript Engine:** Powered by `rquickjs` (QuickJS C runtime) isolated in dedicated background threads. Zero Node.js or Deno needed at runtime.
- **Low-Level Speed:** Built with Axum + Tokio. Sits at ~10-15 MB RAM idle and handles concurrent decryption streams without breaking a sweat.
- **Player Overrides:** Easily force specific YouTube player IDs or variants (like TV/Cobalt) directly from configuration.
- **Automated Upstream Sync:** Built-in `bun run update` script pulls and patches fresh solver routines directly from yt-dlp's upstream repository.

---

## Getting Started

### Requirements
- **Rust** (1.80+) & Cargo
- **Bun** (used during compilation to bundle TypeScript solver routines into `OUT_DIR`)

### Direct Setup
```bash
# 1. Install solver AST dependencies
bun install --production

# 2. Run LavaGuard
cargo run --release
```

### Docker Setup
```bash
# Run prebuilt image from GitHub Container Registry
docker run -d \
  --name lavaguard \
  -p 20015:20015 \
  -v ./config.toml:/app/config.toml \
  ghcr.io/idmja/lavaguard:latest
```

Or using Docker Compose:
```bash
docker compose up -d
```

By default, the server runs on `http://127.0.0.1:20015`.

---

## Configuration (`config.toml`)

```toml
[server]
host = "127.0.0.1"
port = 20015
auth_key = "" # Optional Bearer token

[youtube]
base_url = "https://www.youtube.com"
override_player_variant = "" # e.g. "TV", "IAS", "WEB_EMBED"
override_player_id = ""      # e.g. "ec429320"
```

---

## API Endpoints

### 1. Resolve Stream URL
`POST /resolve_url`

Resolves a raw stream URL by decrypting the signature (`s`) and transforming the `n` parameter.

**Body:**
```json
{
  "stream_url": "https://rr2---sn-4g57kn7e.googlevideo.com/videoplayback?expire=...&n=6wLVP2zMPhCit47nW2wgU",
  "player_url": "https://www.youtube.com/s/player/ec429320/player_embed.vflset/de_DE/base.js"
}
```

**Response:**
```json
{
  "resolved_url": "https://rr2---sn-4g57kn7e.googlevideo.com/videoplayback?expire=...&n=CkSiXRXNSgtJSEeVn0m"
}
```

### 2. Decrypt Signature & N-Param
`POST /decrypt_signature`

Directly deciphers the signature and `n` parameter without needing a full video playback URL.

**Body:**
```json
{
  "player_url": "https://www.youtube.com/s/player/ec429320/player_embed.vflset/de_DE/base.js",
  "encrypted_signature": "AIzaSyD-aW8n_sX7rK_12345",
  "n_param": "6wLVP2zMPhCit47nW2wgU"
}
```

**Response:**
```json
{
  "decrypted_signature": "...",
  "decrypted_n_sig": "CkSiXRXNSgtJSEeVn0m"
}
```

### 3. Extract STS (Signature Timestamp)
`POST /get_sts`

Extracts the signature timestamp from a YouTube player script.

**Body:**
```json
{
  "player_url": "https://www.youtube.com/s/player/ec429320/player_embed.vflset/de_DE/base.js"
}
```

**Response:**
```json
{
  "sts": "20733"
}
```

---

### 4. Generate WebPO / POT (BotGuard)
`POST /generate`

Generates YouTube Proof of Origin Tokens (`poToken`) using an embedded synthetic DOM environment running Google's BotGuard challenge solver.

**Body:**
```json
{
  "content_binding": "visitorData_or_custom_binding",
  "coldToken": false
}
```

**Response:**
```json
{
  "poToken": "...",
  "contentBinding": "...",
  "coldStartToken": null,
  "ttl": 21600
}
```

---

### 5. Decode Cold Start Token
`POST /decode_cold_start`

Decodes a cold start bootstrap token.

**Body:**
```json
{
  "token": "..."
}
```

**Response:**
```json
{
  "contentBinding": "...",
  "timestamp": 1729068000,
  "unknownVal": 0,
  "clientState": 1,
  "keys": [123, 45],
  "date": "2026-10-16T12:00:00.000Z"
}
```

---

## Lavalink Setup

In your Lavalink `application.yml`:

```yaml
plugins:
  youtube:
    remoteCipher:
      url: "http://127.0.0.1:20015"
```

---

## Keeping Solvers Fresh

Whenever YouTube tweaks their player internals, pull the latest solver updates with:

```bash
bun run update
```

This fetches the latest AST matchers, patches them for Rust compatibility, and runs Biome formatting automatically.

---

## Credits & Acknowledgments

LavaGuard stands on the shoulders of continuous reverse engineering efforts from the community:

- **[yt-dlp/ejs](https://github.com/yt-dlp/ejs)**: Core AST-based YouTube signature deciphering routines and player script extraction.
- **[LuanRT/BgUtils](https://github.com/LuanRT/BgUtils)**: Pioneering reverse engineering of YouTube BotGuard and Google Web Anti-Abuse (WAA) challenge verification.
- **[ashton045/webpo-generator](https://github.com/ashton045/webpo-generator)**: Reference HTTP service architecture and challenge fallback strategies.
- **[kikkia/yt-cipher](https://github.com/kikkia/yt-cipher)**: Original remote cipher server concept for Lavalink/NodeLink.
- **[WebReflection/linkedom](https://github.com/WebReflection/linkedom)**: Ultra-fast, lightweight DOM implementation enabling headless browser simulation without Node.js runtime bloat.

---

## License

MIT

