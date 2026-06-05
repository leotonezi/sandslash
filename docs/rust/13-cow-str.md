# `Cow<str>`

## What it is

`Cow<'a, B>` (*Clone on Write*) is an enum in `std::borrow` that holds either a borrowed reference to data (`Borrowed(&'a B)`) or an owned copy of it (`Owned(<B as ToOwned>::Owned)`). For strings the concrete type is `Cow<'_, str>`, which is either `&str` (zero allocation) or `String` (heap allocation). The same type covers both cases, and callers work with it uniformly through `Deref<Target=str>`.

```rust
use std::borrow::Cow;

let b: Cow<str> = Cow::Borrowed("hello");   // &str — no allocation
let o: Cow<str> = Cow::Owned(String::from("hello")); // String — allocated
```

## Why it exists

Many operations *might* need to allocate — or might not, depending on the input. Without `Cow` you are forced to choose one of two bad options:

1. **Always return `String`** — forces an allocation even when the input was already valid and could be returned as-is.
2. **Always return `&str`** — forces the caller to keep the original buffer alive, even when the data was transcoded into a fresh allocation.

`Cow<str>` defers the cost to the cases that actually need it. The common case pays nothing; the exceptional case pays only what is necessary.

A concrete example: `encoding_rs::Encoding::decode()` returns a `Cow<str>`. When the input bytes are valid UTF-8 with no BOM it hands back a `Borrowed` pointing into the original byte slice — zero copy. When the bytes need transcoding (Shift_JIS, Windows-1252, ...) it allocates a new `String` and returns `Owned`. The caller sees `Cow<str>` either way.

## How it works under the hood

### Enum layout

```rust
pub enum Cow<'a, B: ?Sized + 'a>
where
    B: ToOwned,
{
    Borrowed(&'a B),
    Owned(<B as ToOwned>::Owned),
}
```

For `B = str`:
- `Borrowed(&'a str)` — a fat pointer: 16 bytes (pointer + length)
- `Owned(String)` — three words: pointer + length + capacity (24 bytes on 64-bit)

The enum itself is 24 bytes on 64-bit targets (the larger variant dominates) plus a discriminant byte folded into the alignment padding.

### `Deref<Target=str>`

Both variants implement `Deref`:

```rust
impl<'a> Deref for Cow<'a, str> {
    type Target = str;
    fn deref(&self) -> &str {
        match self {
            Cow::Borrowed(s) => s,
            Cow::Owned(s) => s.as_str(),
        }
    }
}
```

This means a `Cow<str>` can be passed anywhere `&str` is expected — method calls, `println!("{cow}")`, `str` slice operations — without any runtime branching visible to the caller.

### `into_owned()` — guaranteed `String`, clone only if needed

```rust
fn into_owned(self) -> <B as ToOwned>::Owned
```

For `Cow<str>` this is `-> String`. It:
- Returns the inner `String` directly if the variant is `Owned` — no copy.
- Calls `s.to_string()` (heap allocation) if the variant is `Borrowed`.

This is the right conversion to use at the point where you need a `String` and want to pay the allocation cost at most once.

### `to_owned()` confusion

`to_owned()` is a method on `&str` (from the `ToOwned` trait), not on `Cow`. Calling `.to_owned()` on a `&str` is equivalent to `String::from(s)` — it always allocates. Calling `.into_owned()` on a `Cow<str>` *may or may not* allocate depending on the variant. Do not confuse the two:

```rust
let s: &str = "hello";
let owned: String = s.to_owned();         // always allocates

let cow: Cow<str> = Cow::Borrowed("hello");
let owned2: String = cow.into_owned();    // allocates (was Borrowed)

let cow2: Cow<str> = Cow::Owned(String::from("hello"));
let owned3: String = cow2.into_owned();   // free — moves the existing String
```

### `Cow::to_mut()` — the actual "clone on write"

`to_mut()` gives a `&mut String`. If the variant is `Borrowed` it clones into `Owned` first. If already `Owned` it just returns `&mut self.0`. This is the lazy-clone pattern the type is named for:

```rust
let mut cow: Cow<str> = Cow::Borrowed("hello");
cow.to_mut().push_str(" world"); // clones once, then mutates in-place
```

This project does not use `to_mut()` — `decode_body` always calls `into_owned()` — but understanding it clarifies why the type is called "Clone on Write".

## This project — where it appears

### `decode_body` — `src/fetcher/client.rs:63–84`

```rust
fn decode_body(bytes: &[u8], ct_header: Option<&str>, url: &Url) -> String {
    // ...charset resolution...
    let (cow, _enc_used, had_errors) = encoding.decode(bytes); // Cow<str>

    if had_errors {
        tracing::warn!(...);
    }

    cow.into_owned()   // line 83: String from Cow — allocates only when needed
}
```

`encoding_rs::Encoding::decode()` returns `(Cow<str>, &Encoding, bool)`. The `Cow<str>`:

- Is `Borrowed` when the bytes are already valid UTF-8 with no BOM. The returned `&str` points directly into `bytes` — the `&[u8]` slice. **No transcoding, no allocation.**
- Is `Owned(String)` when `encoding_rs` had to transcode the bytes (e.g., Shift_JIS, Windows-1252). The `String` was freshly allocated by `encoding_rs`.

At line 83, `cow.into_owned()` converts to the `String` that `decode_body` returns. For the `Owned` variant this is a free move — no second allocation. For the `Borrowed` variant this is the single allocation that produces the `String`.

The return type of `decode_body` is `String`, not `Cow<str>`, because the caller (`fetch` at line 217) stores the result in `PageData::html: String`. The `Cow` is an implementation detail of `encoding_rs`; it is consumed at the boundary and does not leak into the wider API.

### `charset_from_meta_sniff` — `src/fetcher/client.rs:36–54`

```rust
let snippet = String::from_utf8_lossy(&bytes[..sniff_len]).to_ascii_lowercase();
```

`String::from_utf8_lossy` also returns a `Cow<str>`. If the byte slice is valid UTF-8 it returns `Borrowed(&str)` into the slice; otherwise it returns `Owned(String)` with replacement characters. Here `.to_ascii_lowercase()` is called immediately, which allocates regardless — but the pattern is common enough to know: `from_utf8_lossy` is itself a zero-copy fast path.

### Test fixtures — `src/fetcher/client.rs:509`

```rust
let (encoded, _, _) = encoding_rs::SHIFT_JIS.encode(html_str);
let body_bytes = encoded.into_owned();
```

`encoding_rs::Encoding::encode()` also returns `Cow<[u8]>` — the same borrowing pattern for byte slices. `into_owned()` forces a `Vec<u8>` so the test can pass `body_bytes` as owned data to `wiremock`. This mirrors the `decode` usage: `into_owned()` at the boundary where ownership is required.

## Common mistakes

### Calling `.to_string()` or `.to_owned()` on the `Cow` instead of `.into_owned()`

```rust
// Wasteful: always allocates, even when cow is already Owned
let s: String = cow.to_string();

// Correct: moves if Owned, clones if Borrowed
let s: String = cow.into_owned();
```

### Holding `Cow<str>` backed by a local `&[u8]` across an `.await`

The `Borrowed` variant of `Cow<str>` carries a lifetime `'a` tied to the bytes it borrows from. If those bytes come from a local `Vec<u8>` or `Bytes`, the `Cow` cannot outlive that local binding. Because futures must be `'static` to cross `.await` points in most `tokio::spawn` contexts, a borrowed `Cow` cannot be held across `.await`.

`decode_body` avoids this entirely by converting to `String` (via `into_owned()`) before returning. The `Cow` is a transient value inside the function body; the `String` is what escapes.

### Using `Cow<str>` in `PageData` fields

`PageData::html` is `String`, not `Cow<str>`. Making it `Cow<'_, str>` would infect every type that holds `PageData` with a lifetime parameter, requiring annotation on `Arc<PageData>`, async tasks, and `#[derive(Clone)]`. The cost is not worth the potential allocation saving. `Cow<str>` is most valuable in function *parameters* and short-lived intermediate values, not in long-lived stored structs.

### Forgetting that `Deref` auto-coercion works

```rust
fn process(s: &str) { ... }

let cow: Cow<str> = Cow::Borrowed("hello");
process(&cow);   // works — Deref coerces Cow<str> to &str
process(&*cow);  // explicit deref — same thing
```

Both are correct. Prefer `&cow` — it is shorter and the compiler handles the double-deref.

### `Cow<str>` vs `&str` in function signatures

```rust
// Use &str when you only ever read and never need to own
fn length(s: &str) -> usize { s.len() }

// Use Cow<str> when you MAY need to allocate (e.g., conditional escaping)
fn escape_if_needed(s: &str) -> Cow<str> {
    if s.contains('<') {
        Cow::Owned(s.replace('<', "&lt;"))
    } else {
        Cow::Borrowed(s)
    }
}
```

If the function always returns a `String`, return `String`. If it always reads, take `&str`. Use `Cow<str>` only at the boundary where the allocation is genuinely conditional.

## Quick reference

| Expression | Type | Allocates? |
|---|---|---|
| `Cow::Borrowed("hello")` | `Cow<str>` | No |
| `Cow::Owned(String::from("hello"))` | `Cow<str>` | Yes |
| `encoding.decode(bytes).0` | `Cow<str>` | Only if transcoding needed |
| `String::from_utf8_lossy(bytes)` | `Cow<str>` | Only if invalid UTF-8 |
| `cow.into_owned()` | `String` | Only if `Borrowed` |
| `cow.to_string()` | `String` | Always |
| `s.to_owned()` on `&str` | `String` | Always |
| `&cow` | `&str` | Never |
| `cow.to_mut()` | `&mut String` | Only if `Borrowed` |

When to use `Cow<str>`:

| Scenario | Pattern |
|---|---|
| Return value that is sometimes already a `&str`, sometimes needs allocation | Return `Cow<str>` |
| Function parameter that accepts both `&str` and `String` | Take `impl Into<Cow<str>>` or just `&str` |
| Short-lived transcoding result consumed immediately | `Cow<str>` from `encoding_rs`, then `into_owned()` at the boundary |
| Long-lived field in a struct | Prefer `String` — lifetime on `Cow` infects the struct |
| Read-only string operation | `&str` — simpler, always correct |
