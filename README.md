# Particle Physics - Rust + Leptos Full-Stack Web App

Interactive particle simulation running at 60fps in the browser, compiled from Rust to WebAssembly via Leptos 0.8. It handles 2,500 particles: a spatial grid finds the links, lines are drawn in colour batches, and glows come from pre-rendered sprites with additive blending. The canvas renders sharply on HiDPI screens.

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
|                                   | Particles  300|        |
|                                   | Link Range 115|        |
|                                   | Speed      1.0|        |
|                                   | Trails     62%|        |
|                                   | [Neon][Fire][Ocean]   |
|                                   |  [+ Attract]  |        |
|                                   +--------------+        |
+-----------------------------------------------------------+
```

**Open at:** http://localhost:8080 (Docker) or http://127.0.0.1:3000 (`cargo leptos serve`)

---

## Reactor mode

Switch the panel to **☢ Reactor** to run a toy fission chain reaction:

- **Fuel nuclei** (green) jostle and bounce off each other. When a **neutron** (blue streak) hits one, it may split. The nucleus is flung away glowing hot and turns grey (spent) until it refuels, and 2–3 new neutrons fly out. Hot nuclei hit their neighbours harder and pass the glow on.
- Neutrons that miss lose speed with each bounce (moderation), and slow neutrons are much likelier to cause fission. Spent nuclei sometimes swallow neutrons (poisoning), and neutrons that reach the screen edge leak away.
- **Control rods** hang from the top and absorb neutrons. Lowering them is how you slow the reaction. **SCRAM** drops them all the way in at once.
- The HUD shows status (shutdown / subcritical / critical / supercritical), core temperature, power in fissions per second, neutron count and the actual rod position. The blue Cherenkov glow behind the core brightens with power.
- Above 1200°C the core **melts down**: alarms flash, the screen shakes and the reactor auto-SCRAMs. You can't reset it until the core cools below 700°C.
- Click anywhere to fire a burst of neutrons.
- **Coolant pump:** more flow cools the core faster but uses up to 12% of the electrical output.
- **Xenon-135 poisoning:** fission makes iodine, which decays into neutron-absorbing xenon. Running at power burns xenon off. Cutting power stops the burn-off, so xenon piles up and the core goes sluggish for a while (the "xenon pit").

### Operator shift (the game)

Press **▶ Start shift** in the reactor HUD to run the plant for a compressed day, 06:00 to 06:00 in three minutes, starting from a cold core:

- City demand follows a daily curve: about 140 MW overnight, a morning ramp, and about 500 MW at the evening peak, which needs the pump opened up and the rods pulled. A green band on the Output gauge shows the target.
- Every second on target scores points. The multiplier grows by ×1 for every 10 seconds you stay in the band, up to ×5, and drops back to ×1 when you leave it.
- **Incidents** hit every 22–35 seconds:
  - *coolant pump failure*: cooling drops to 35%;
  - *jammed control rod*: drawn in orange;
  - *heatwave demand surge*: +130 MW;
  - *turbine trip*: demand falls to 35%, which sets up a xenon pit.
- A meltdown ends the shift and halves the score. The end card shows a star rating based on time on target. Your best score is saved in this browser (`localStorage`).
- **Sound** (🔊 in the top bar) starts after your first click: Geiger clicks follow the neutron count, the hum rises with power, a klaxon sounds during meltdown, the rods thunk on SCRAM, and a chime marks each incident.

The physics (`src/reactor.rs`) and the shift rules (`src/shift.rs`) use no web APIs, so they are unit tested natively (`cargo test --lib --features ssr`):
- **Criticality:** withdrawn rods melt the core down, half-inserted rods hold a steady reaction, and fully inserted rods shut it down.
- **Xenon:** xenon piles up after shutdown.
- **Pump failure:** the core runs hotter.
- **Shift:** incidents apply and clear, and the multiplier and demand curve behave.

Sound lives in `src/audio.rs` (WebAudio, browser only).

---

## Run with Docker Compose (easiest)

No local Rust toolchain needed:

```bash
docker compose up -d --build
```

Open http://localhost:8080. The first build takes a while because it compiles `cargo-leptos`, the server binary and the WASM bundle. Later rebuilds reuse the cached `cargo-leptos` layer.

| Command | What it does |
|---------|-------------|
| `docker compose up -d --build` | Build the image and start the container |
| `docker compose logs -f` | Follow server logs (should show `listening on http://0.0.0.0:3000`) |
| `docker compose down` | Stop and remove the container |

How the image works:
- **Build stage** (`rust:1.99-slim-bookworm`) builds `cargo-leptos` 0.3.11 from source, adds the `wasm32-unknown-unknown` target and runs `cargo leptos build --release`. LTO is turned off for both `cargo-leptos` and the `wasm-release` profile so the build fits in an 8 GB Docker VM. The WASM is a little larger as a result.
- **Runtime stage** (`debian:bookworm-slim`) holds only the server binary and `target/site/`, and runs as a non-root `appuser`.
- **Port:** the container listens on 3000, and compose publishes it on host port **8080**.
- **Config:** the binary reads `LEPTOS_*` environment variables at startup (`get_configuration(None)`). The `[package.metadata.leptos]` section of `Cargo.toml` only applies under `cargo leptos`. The Dockerfile sets `LEPTOS_SITE_ADDR=0.0.0.0:3000` and `LEPTOS_ENV=PROD`. Without those, the server binds `127.0.0.1` inside the container, which can't be reached from the host, and it injects the dev live-reload script.

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
  Dockerfile              # Multi-stage build: cargo-leptos build -> slim runtime
  docker-compose.yml      # Publishes the app on localhost:8080
  .dockerignore
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

// Stub so SSR binary compiles cleanly (returns false = not started)
#[cfg(not(feature = "hydrate"))]
fn try_start(...) -> bool { false }
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
site-addr = "0.0.0.0:3000"
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
| Docker container is "healthy" but `localhost:8080` resets the connection | Server bound `127.0.0.1` because no `LEPTOS_SITE_ADDR` was set at runtime (the healthcheck runs inside the container, so it still passes) | Set `LEPTOS_SITE_ADDR=0.0.0.0:3000` in the runtime image (already done in the Dockerfile) |
| Docker build killed / OOM during linking | `lto = true` + `codegen-units = 1` on a large dependency tree | Turn LTO off (see the Dockerfile overrides) or give Docker more memory |
| Browser shows a different app | Something else is already using port 3000 on the host | Use http://localhost:8080 for the Docker build, or change the host port in `docker-compose.yml` |
| Old page after a rebuild | `/pkg/*` filenames aren't hashed, so the browser cached the old files | Hard refresh (Cmd/Ctrl+Shift+R) |
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
- `hue_range()` / `bucket_hue()` / `line_styles()` - palette quantized into hue/alpha buckets so colours are built once per palette
- `build_sprites()` - pre-rendered radial-gradient glow per hue bucket (replaces `shadowBlur`)
- `start_simulation()` - WASM-only animation loop (Closure + requestAnimationFrame): HiDPI sizing, spatial-grid link search, batched line strokes, additive blending, cursor glow ring
- `new_particle()` - random particle constructor
- `draw_reactor()` - renders rods, nuclei, fission glow, shock rings and neutron streaks

### src/reactor.rs / src/grid.rs

- `Reactor` - fission chain-reaction physics (nuclei, neutrons, control rods, temperature, SCRAM/meltdown)
- `Grid` - reusable spatial hash shared by the plexus links and reactor collisions
- `Shift` (`src/shift.rs`) - demand curve, scoring, multiplier and incidents
- `Sound` (`src/audio.rs`) - Geiger clicks, hum, klaxon, SCRAM thunk and incident chime

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
