# seo-rs web UI

A minimal Next.js 14 front-end for the seo-rs Rust CLI.

## Prerequisites

1. **Build the Rust binary** (from the repo root):
   ```bash
   cargo build --release
   ```
   This produces `target/release/seo-rs`.

2. **Install Node dependencies** (from this directory):
   ```bash
   npm install
   ```

## Running

```bash
npm run dev
```

Open [http://localhost:3000](http://localhost:3000), paste a URL, and click **Run Audit**.

## Binary path

By default the API route looks for the binary at `../target/release/seo-rs` relative to the
`frontend/` working directory. Override with the `SEO_RS_BIN` environment variable:

```bash
SEO_RS_BIN=/usr/local/bin/seo-rs npm run dev
```

## Other commands

| Command | Description |
|---|---|
| `npm run build` | Production build |
| `npm run start` | Start production server |
| `npm run lint` | Run ESLint |
