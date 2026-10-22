# Particle Physics - Rust + Leptos Full-Stack Web App

Interactive particle simulation running at 60fps in the browser, compiled from Rust to WebAssembly via Leptos 0.8.

```
+-----------------------------------------------------------+
|  Rust + Leptos                                  60 fps    |
|                                                           |
|      .--.        .              .                         |
|     /  . /  .     \  .       .--.     /\                |
|    .----X----- .    .---/    \     /\  \   .--.            |
|     \  . \  .     /  . /     \     /\  \                  |
|      .--.        .           .--.                         |
|                                                           |
|                                   +--------------+        |
|                                   | Particles  160|        |
|                                   | Link Range 115|        |
|                                   | Speed      1.0|        |
|                                   | [Neon][Fire][Ocean]   |
|                                   |  [+ Attract]  |        |
|                                   +--------------+        |
+-----------------------------------------------------------+
```

**Open at:** http://127.0.0.1:3000

---

## Architecture

```
+--------------------------------------------------------------+
|  BROWSER                                                      |
|  +---------------------+  +--------------------------------+ |
|  |  SSR HTML (initial) |  |  WASM Module (hydrate)         | |
|  |  Rendered by Axum   |  |  Rust compiled to wasm32       | |
|  |  Static page shell  |--|  -> Canvas animation loop      | |
|  |                     |  |  -> Mouse event handlers       | |
|  |                     |  |  -> Leptos reactive signals    | |
|  +---------------------+  +--------------------------------+ |
+--------------------------------------------------------------+
```

**Dual build model:** Leptos compiles the same source **twice**:
- `--features ssr` -> native binary (Axum HTTP server rendering HTML)
- `--features hydrate` -> WASM bundle (interactive client-side logic)

The SSR server sends pre-rendered HTML; the WASM module hydrates it with interactivity. `cargo-leptos` orchestrates both builds and the dev server automatically.

---

## Prerequisites

| Tool | Version | Purpose | Install |
|------|---------|---------|----------|
| rustup | latest stable | Rust toolchain installer | https://rustup.rs |
| wasm32-unknown-unknown | (target) | Compile Rust to WASM | `rustup target add wasm32-unknown-unknown` |
| MSVC Linker | VS 2022 Build Tools | Link native binaries (Windows) | `winget install Microsoft.VisualStudio.2022.BuildTools` |
| cargo-leptos | >= 0.3 | Build orchestrator + dev server | `cargo install cargo-leptos --locked` |

### Platform-specific C/C++ toolchain

```bash
# Windows
winget install "Microsoft.VisualStudio.2022.BuildTools" --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"

# macOS
xcode-select --install

# Linux (Debian/Ubuntu)
sudo apt install build-essential
```

---

## Quick Start (full setup from scratch)

```bash
# 1. Install Rust (skip if already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
# Windows: download rustup-init.exe from https://win.rustup.rs/x86_64
# Then: .\rustup-init.exe -y

# 2. Add WASM target
rustup target add wasm32-unknown-unknown

# 3. Install cargo-leptos (compiles from source, ~7 min)
cargo install cargo-leptos --locked

# 4. Scaffold project (optional - or clone this repo)
cargo leptos new --git leptos-rs/start-axum my-leptos-app

# 5. Replace files with source from this repo (see File Contents below)

# 6. Run dev server with hot-reload
cd my-leptos-app
cargo leptos serve
```

Open http://127.0.0.1:3000

---

## Build Commands

| Command | What it does |
|---------|-------------|
| `cargo leptos serve` | Dev build + hot-reload server at :3000 |
| `cargo leptos build` | Production build (optimized WASM + binary) |
| `cargo leptos build --release` | Full release with wasm-release profile |
| `cargo leptos watch` | Alias for serve with file watching |

Production output goes to `target/site/`.

---

## Project Structure

```
my-leptos-app/
  Cargo.toml              # Dependencies, features, leptos config
  rust-toolchain.toml     # Pins stable toolchain
  .gitignore
  src/
    app.rs                # Components, signals, WASM simulation engine
    lib.rs                # Crate root + WASM hydrate entry point
    main.rs               # SSR Axum server entry point
  style/
    main.scss             # SCSS compiled by cargo-leptos (dart-sass)
  public/                 # Static assets (favicon, etc.)
```

---

## Dependencies Explained

| Crate | Feature Gate | Purpose |
|-------|-------------|----------|
| `leptos` | always | Reactive signals, components, hydration |
| `leptos_router` | always | Client-side routing |
| `leptos_meta` | always | Head management (title, stylesheet, meta) |
| `axum` | `ssr` only | HTTP server for SSR |
| `tokio` | `ssr` only | Async runtime for the server |
| `leptos_axum` | `ssr` only | Axum <-> Leptos integration |
| `wasm-bindgen` | `hydrate` only | Rust <-> JS interop for WASM |
| `web-sys` | `hydrate` only | Typed browser API bindings (canvas, events) |
| `js-sys` | `hydrate` only | JS runtime access (Math::random()) |
| `console_error_panic_hook` | `hydrate` only | Nice panic messages in browser console |

### Why web-sys features must be declared explicitly

Leptos internally depends on web-sys but only enables a minimal feature set. To use CanvasRenderingContext2d, you must add it to your own [dependencies.web-sys] section in Cargo.toml. Without this, you get:

```
error[E0599]: no method named `get_context` found for struct `HtmlCanvasElement`
```

---

## Key Code Patterns

These patterns are essential to understand for replication:

### 1. Feature-gating browser code

```rust
// This compiles ONLY for WASM builds (cargo leptos build --lib)
#[cfg(feature = "hydrate")]
fn start_simulation(...) {
    use web_sys::CanvasRenderingContext2d;
    // ...
}

// Stub so SSR binary compiles cleanly
#[cfg(not(feature = "hydrate"))]
fn try_start(...) {}
```

### 2. NodeRef for DOM element access

```rust
let canvas_ref = NodeRef::<Canvas>::new();
view! { <canvas node_ref=canvas_ref /> }

// In Effect (runs after DOM mount):
if let Some(el) = canvas_ref.get() {
    let canvas: &HtmlCanvasElement = el.as_ref();
}
```

### 3. Signals <-> shared state bridge

Reactive signals drive the UI. For the animation loop (outside reactive context), use Rc<RefCell<T>>:

```rust
let shared = Rc::new(RefCell::new(SimSettings::default()));
let s_sync = shared.clone();

Effect::new(move |_| {
    // Every time a signal changes, sync to shared state
    *s_sync.borrow_mut() = SimSettings { count: count.get(), ... };
});
// Animation closure reads from shared.borrow() each frame
```

### 4. requestAnimationFrame loop with Rust closures

```rust
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;

let closure: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> =
    Rc::new(RefCell::new(None));
let closure_ref = closure.clone();

*closure.borrow_mut() = Some(Closure::wrap(Box::new(move |time: f64| {
    // ... draw frame ...
    // Request next frame (keeps loop alive via Rc)
    if let Some(c) = closure_ref.borrow().as_ref() {
        let _ = win.request_animation_frame(c.as_ref().unchecked_ref());
    }
}) as Box<dyn FnMut(f64)>));

// Kick off the loop
let _ = web_sys::window().unwrap().request_animation_frame(
    closure.borrow().as_ref().unwrap().as_ref().unchecked_ref()
);
```

**Critical:** Do NOT move `win` into the closure and then use it outside. Call `web_sys::window().unwrap()` again for the initial request.

### 5. SSR shell function

The `shell()` function produces the full HTML document with DOCTYPE - this is what the SSR server sends before hydration:

```rust
pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <AutoReload options=options.clone() />
                <HydrationScripts options />  // loads the WASM module
                <MetaTags />
            </head>
            <body><App/></body>
        </html>
    }
}
```

---

## Cargo.toml (complete)

```toml
[package]
name = "my_leptos_app"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
leptos = { version = "0.8.0" }
leptos_router = { version = "0.8.0" }
axum = { version = "0.8.0", optional = true }
console_error_panic_hook = { version = "0.1", optional = true }
leptos_axum = { version = "0.8.0", optional = true }
leptos_meta = { version = "0.8.0" }
tokio = { version = "1", features = ["rt-multi-thread"], optional = true }
wasm-bindgen = { version = "0.2.106", optional = true }
js-sys = { version = "0.3", optional = true }
gloo-timers = { version = "0.3", optional = true }

[dependencies.web-sys]
version = "0.3"
optional = true
features = [
    "CanvasRenderingContext2d",
    "HtmlCanvasElement",
    "Window",
    "MouseEvent",
    "Element",
]

[features]
hydrate = [
    "leptos/hydrate",
    "dep:console_error_panic_hook",
    "dep:wasm-bindgen",
    "dep:js-sys",
    "dep:web-sys",
    "dep:gloo-timers",
]
ssr = [
    "dep:axum",
    "dep:tokio",
    "dep:leptos_axum",
    "leptos/ssr",
    "leptos_meta/ssr",
    "leptos_router/ssr",
]

[profile.wasm-release]
inherits = "release"
opt-level = 'z'
lto = true
codegen-units = 1
panic = "abort"

[package.metadata.leptos]
output-name = "my_leptos_app"
site-root = "target/site"
site-pkg-dir = "pkg"
style-file = "style/main.scss"
assets-dir = "public"
site-addr = "127.0.0.1:3000"
reload-port = 3001
browserquery = "defaults"
env = "DEV"
bin-features = ["ssr"]
bin-default-features = false
lib-features = ["hydrate"]
lib-default-features = false
lib-profile-release = "wasm-release"
```

---

## Common Pitfalls & Fixes

| Error | Cause | Fix |
|-------|-------|-----|
| `cannot find module or crate gloo_timers` | Browser-only crate used in SSR context | Remove it, gate with `#[cfg(feature = "hydrate")]`, or use CSS animations instead |
| `no method named get_context` | Missing web-sys features in Cargo.toml | Add CanvasRenderingContext2d, HtmlCanvasElement to web-sys features |
| `borrow of moved value: win` | web_sys::Window moved into move closure, then used outside | Use web_sys::window().unwrap() for the second call instead of reusing win |
| `set_untracked` not found | Method does not exist in Leptos 0.8 RwSignal | Use .set() - works fine outside reactive context |
| `no method inner_width on Window` | Missing Window in web-sys features | Add Window to [dependencies.web-sys] features |
| `js-sys` / `web-sys` not found at SSR build | Not marked optional = true + not in hydrate feature only | Add optional = true and put them in hydrate feature list only |
| `Finished generating JS/WASM` but page shows old content | Server binary locked | Kill process: `Get-Process *my_leptos* | Stop-Process -Force` then restart |

---

## Styling Notes

- SCSS file is at `style/main.scss` - compiled by dart-sass (bundled with cargo-leptos).
- cargo-leptos hot-reloads CSS changes without a full rebuild.
- Canvas uses semi-transparent fill (`rgba(8,8,18,0.22)`) for motion trail effect instead of `clearRect()`.
- Control panel uses `backdrop-filter: blur()` for glassmorphism.
- Hint pill uses CSS animation (`hintFade`) to auto-hide after 4 seconds.

---

## File Contents

### src/lib.rs

```rust
pub mod app;

#[cfg(feature = "hydrate")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn hydrate() {
    use crate::app::*;
    console_error_panic_hook::set_once();
    leptos::mount::hydrate_body(App);
}
```

### src/main.rs

```rust
#[cfg(feature = "ssr")]
#[tokio::main]
async fn main() {
    use axum::Router;
    use leptos::logging::log;
    use leptos::prelude::*;
    use leptos_axum::{generate_route_list, LeptosRoutes};
    use my_leptos_app::app::*;

    let conf = get_configuration(None).unwrap();
    let addr = conf.leptos_options.site_addr;
    let leptos_options = conf.leptos_options;
    let routes = generate_route_list(App);

    let app = Router::new()
        .leptos_routes(&leptos_options, routes, {
            let leptos_options = leptos_options.clone();
            move || shell(leptos_options.clone())
        })
        .fallback(leptos_axum::file_and_error_handler(shell))
        .with_state(leptos_options);

    log!("listening on http://{}", &addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service()).await.unwrap();
}

#[cfg(not(feature = "ssr"))]
pub fn main() {}
```

### src/app.rs

See file in repository. Contains:
- `shell()` - HTML document wrapper
- `App` component - router + stylesheet injection
- `HomePage` component - canvas + controls + signals
- `try_start()` - cfg-gated animation starter
- `start_simulation()` - WASM-only animation loop (Closure + requestAnimationFrame)
- `new_particle()` - random particle constructor

---

## References

- [Leptos Book](https://book.leptos.dev) - official guide
- [cargo-leptos GitHub](https://github.com/leptos-rs/cargo-leptos) - build tool docs
- [wasm-bindgen Guide](https://rustwasm.github.io/wasm-bindgen/)
- [web-sys crate docs](https://rustwasm.github.io/wasm-bindgen/api/web_sys/)
- [Leptos Start Template](https://github.com/leptos-rs/start-axum) - scaffold this project from

---

## License

MIT
