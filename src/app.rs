use leptos::prelude::*;
use leptos::html::Canvas;
use leptos_meta::{provide_meta_context, MetaTags, Stylesheet, Title};
use leptos_router::{
    components::{Route, Router, Routes},
    StaticSegment,
};
use std::cell::RefCell;
use std::rc::Rc;

pub fn shell(options: LeptosOptions) -> impl IntoView {
    view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8"/>
                <meta name="viewport" content="width=device-width, initial-scale=1"/>
                <AutoReload options=options.clone() />
                <HydrationScripts options/>
                <MetaTags/>
            </head>
            <body><App/></body>
        </html>
    }
}

#[component]
pub fn App() -> impl IntoView {
    provide_meta_context();
    view! {
        <Stylesheet id="leptos" href="/pkg/my_leptos_app.css"/>
        <Title text="Particle Sim • Rust + Leptos"/>
        <Router>
            <main>
                <Routes fallback=|| "Page not found.".into_view()>
                    <Route path=StaticSegment("") view=HomePage/>
                </Routes>
            </main>
        </Router>
    }
}

#[derive(Clone)]
pub struct SimSettings {
    pub count: usize,
    pub conn_dist: f64,
    pub speed: f64,
    pub color_scheme: u8,
    pub repel: bool,
}
impl Default for SimSettings {
    fn default() -> Self {
        Self { count: 160, conn_dist: 115.0, speed: 1.0, color_scheme: 0, repel: false }
    }
}

#[derive(Clone, Copy)]
pub struct Particle {
    pub x: f64, pub y: f64,
    pub vx: f64, pub vy: f64,
    pub hue: f64,
    pub size: f64,
}

#[component]
fn HomePage() -> impl IntoView {
    let canvas_ref = NodeRef::<Canvas>::new();
    let count        = RwSignal::new(160usize);
    let conn_dist    = RwSignal::new(115f64);
    let speed        = RwSignal::new(1.0f64);
    let color_scheme = RwSignal::new(0u8);
    let is_repel     = RwSignal::new(false);
    let fps          = RwSignal::new(0u32);
    let hint_visible = RwSignal::new(true);

    let shared: Rc<RefCell<SimSettings>> = Rc::new(RefCell::new(SimSettings::default()));
    let mouse:  Rc<RefCell<Option<(f64,f64)>>> = Rc::new(RefCell::new(None));

    let s_sync = shared.clone();
    Effect::new(move |_| {
        *s_sync.borrow_mut() = SimSettings {
            count: count.get(), conn_dist: conn_dist.get(),
            speed: speed.get(), color_scheme: color_scheme.get(),
            repel: is_repel.get(),
        };
    });

    let s_start = shared.clone();
    let m_start = mouse.clone();
    Effect::new(move |_| {
        try_start(canvas_ref, s_start.clone(), m_start.clone(), fps);
    });


    let m_move  = mouse.clone();
    let m_leave = mouse.clone();

    view! {
        <div class="sim-root">
            <canvas node_ref=canvas_ref class="particle-canvas"
                on:mousemove=move |ev| {
                    *m_move.borrow_mut() = Some((ev.client_x() as f64, ev.client_y() as f64));
                }
                on:mouseleave=move |_| { *m_leave.borrow_mut() = None; }
            />
            <div class="hud-bar">
                <h1 class="hud-title">"🦀 Particle Physics"</h1>
                <div class="fps-badge">{move || format!("{} fps", fps.get())}</div>
            </div>
            <div class="ctrl-panel">
                <p class="ctrl-heading">"Controls"</p>
                <div class="ctrl-row">
                    <div class="ctrl-label">
                        <span>"Particles"</span>
                        <span class="ctrl-val">{move || count.get()}</span>
                    </div>
                    <input class="slider" type="range" min="40" max="400" step="5" value="160"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<usize>() { count.set(v); }
                        }
                    />
                </div>
                <div class="ctrl-row">
                    <div class="ctrl-label">
                        <span>"Link Range"</span>
                        <span class="ctrl-val">{move || format!("{}px", conn_dist.get() as u32)}</span>
                    </div>
                    <input class="slider" type="range" min="40" max="220" step="5" value="115"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() { conn_dist.set(v); }
                        }
                    />
                </div>
                <div class="ctrl-row">
                    <div class="ctrl-label">
                        <span>"Speed"</span>
                        <span class="ctrl-val">{move || format!("{:.1}x", speed.get())}</span>
                    </div>
                    <input class="slider" type="range" min="1" max="30" step="1" value="10"
                        on:input=move |ev| {
                            if let Ok(v) = event_target_value(&ev).parse::<f64>() { speed.set(v / 10.0); }
                        }
                    />
                </div>
                <div class="ctrl-row">
                    <span class="ctrl-label-single">"Palette"</span>
                    <div class="palette-row">
                        <button class="pal-btn pal-neon"    on:click=move |_| color_scheme.set(0)>"◈ Neon"</button>
                        <button class="pal-btn pal-fire"    on:click=move |_| color_scheme.set(1)>"🔥 Fire"</button>
                        <button class="pal-btn pal-ocean"   on:click=move |_| color_scheme.set(2)>"🌊 Ocean"</button>
                        <button class="pal-btn pal-rainbow" on:click=move |_| color_scheme.set(3)>"🌈 Rnbw"</button>
                    </div>
                </div>
                <div class="ctrl-row">
                    <button class="mode-btn" class-repel=move || is_repel.get()
                        on:click=move |_| is_repel.update(|r| *r = !*r)
                    >{move || if is_repel.get() { "⊖ Repel" } else { "⊕ Attract" }}</button>
                </div>
            </div>
            <Show when=move || hint_visible.get()>
                <div class="hint-pill">"🖱 Move your mouse to interact"</div>
            </Show>
        </div>
    }
}

// ── Entry point: starts animation only when canvas is available (hydrate) ─
#[cfg(feature = "hydrate")]
fn try_start(
    canvas_ref: NodeRef<Canvas>,
    shared: Rc<RefCell<SimSettings>>,
    mouse: Rc<RefCell<Option<(f64, f64)>>>,
    fps: RwSignal<u32>,
) {
    if let Some(el) = canvas_ref.get() {
        use web_sys::HtmlCanvasElement;
        let canvas: &HtmlCanvasElement = el.as_ref();
        start_simulation(canvas.clone(), shared, mouse, fps);
    }
}

#[cfg(not(feature = "hydrate"))]
fn try_start(
    _canvas_ref: NodeRef<Canvas>,
    _shared: Rc<RefCell<SimSettings>>,
    _mouse: Rc<RefCell<Option<(f64, f64)>>>,
    _fps: RwSignal<u32>,
) {}

// ── Color helpers ─────────────────────────────────────────────────────────
fn particle_color(hue: f64, alpha: f64) -> String {
    format!("hsla({:.0}, 100%, 70%, {:.2})", hue, alpha)
}
fn line_color(hue: f64, alpha: f64) -> String {
    format!("hsla({:.0}, 80%, 65%, {:.3})", hue, alpha)
}

// ── WASM simulation (hydrate only) ────────────────────────────────────────
#[cfg(feature = "hydrate")]
fn start_simulation(
    canvas: web_sys::HtmlCanvasElement,
    shared: Rc<RefCell<SimSettings>>,
    mouse: Rc<RefCell<Option<(f64, f64)>>>,
    fps_signal: RwSignal<u32>,
) {
    use wasm_bindgen::closure::Closure;
    use wasm_bindgen::{JsCast, JsValue};
    use web_sys::CanvasRenderingContext2d;

    let win = web_sys::window().unwrap();
    let w = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
    let h = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);
    canvas.set_width(w as u32);
    canvas.set_height(h as u32);

    let ctx: CanvasRenderingContext2d =
        canvas.get_context("2d").unwrap().unwrap().unchecked_into();

    let init_settings = shared.borrow().clone();
    let mut particles: Vec<Particle> =
        (0..init_settings.count).map(|_| new_particle(w, h, init_settings.color_scheme)).collect();

    let mut last_time: f64 = 0.0;
    let mut frames: u32 = 0;
    let mut last_fps: f64 = 0.0;

    let shared_c = shared.clone();
    let mouse_c  = mouse.clone();
    let canvas_c = canvas.clone();

    let closure: Rc<RefCell<Option<Closure<dyn FnMut(f64)>>>> =
        Rc::new(RefCell::new(None));
    let closure_ref = closure.clone();

    *closure.borrow_mut() = Some(Closure::wrap(Box::new(move |time: f64| {
        let settings = shared_c.borrow().clone();
        let mouse_data = *mouse_c.borrow();

        let cw = win.inner_width().unwrap().as_f64().unwrap_or(1920.0);
        let ch = win.inner_height().unwrap().as_f64().unwrap_or(1080.0);
        if canvas_c.width() != cw as u32 || canvas_c.height() != ch as u32 {
            canvas_c.set_width(cw as u32);
            canvas_c.set_height(ch as u32);
        }

        // FPS
        frames += 1;
        if time - last_fps > 1000.0 {
            fps_signal.set(frames);
            frames = 0;
            last_fps = time;
        }

        let dt = if last_time > 0.0 { ((time - last_time) / 16.667).min(3.0) } else { 1.0 };
        last_time = time;

        // Adjust count
        while particles.len() < settings.count {
            particles.push(new_particle(cw, ch, settings.color_scheme));
        }
        particles.truncate(settings.count);

        // ── Clear with slight trail ──
        ctx.set_shadow_blur(0.0);
        ctx.set_fill_style(&JsValue::from_str("rgba(8,8,18,0.22)"));
        ctx.fill_rect(0.0, 0.0, cw, ch);

        // ── Update particles ──
        for p in particles.iter_mut() {
            if let Some((mx, my)) = mouse_data {
                let dx = mx - p.x;
                let dy = my - p.y;
                let d2 = dx * dx + dy * dy;
                if d2 < 180.0 * 180.0 && d2 > 4.0 {
                    let d = d2.sqrt();
                    let f = 600.0 / d2;
                    let sign = if settings.repel { -1.0 } else { 1.0 };
                    p.vx += sign * (dx / d) * f;
                    p.vy += sign * (dy / d) * f;
                }
            }
            p.vx *= 0.975;
            p.vy *= 0.975;
            let spd = (p.vx * p.vx + p.vy * p.vy).sqrt();
            let max = 4.5 * settings.speed;
            if spd > max { p.vx = (p.vx / spd) * max; p.vy = (p.vy / spd) * max; }
            p.vx += (js_sys::Math::random() - 0.5) * 0.28 * settings.speed;
            p.vy += (js_sys::Math::random() - 0.5) * 0.28 * settings.speed;
            p.x += p.vx * dt * settings.speed;
            p.y += p.vy * dt * settings.speed;
            if p.x < 0.0  { p.x += cw; } if p.x > cw { p.x -= cw; }
            if p.y < 0.0  { p.y += ch; } if p.y > ch { p.y -= ch; }
        }

        // ── Draw plexus lines ──
        ctx.set_line_width(0.6);
        let cd2 = settings.conn_dist * settings.conn_dist;
        for i in 0..particles.len() {
            for j in (i + 1)..particles.len() {
                let dx = particles[i].x - particles[j].x;
                let dy = particles[i].y - particles[j].y;
                let d2 = dx * dx + dy * dy;
                if d2 < cd2 {
                    let alpha = (1.0 - d2 / cd2) * 0.45;
                    let hue = (particles[i].hue + particles[j].hue) * 0.5;
                    ctx.set_shadow_blur(0.0);
                    ctx.set_stroke_style(&JsValue::from_str(&line_color(hue, alpha)));
                    ctx.begin_path();
                    ctx.move_to(particles[i].x, particles[i].y);
                    ctx.line_to(particles[j].x, particles[j].y);
                    ctx.stroke();
                }
            }
        }

        // ── Draw particles with glow ──
        for p in particles.iter() {
            let col = particle_color(p.hue, 0.95);
            ctx.set_shadow_blur(p.size * 7.0);
            ctx.set_shadow_color(&col);
            ctx.set_fill_style(&JsValue::from_str(&col));
            ctx.begin_path();
            ctx.arc(p.x, p.y, p.size, 0.0, std::f64::consts::TAU);
            ctx.fill();
        }

        // Request next frame
        if let Some(c) = closure_ref.borrow().as_ref() {
            let _ = win.request_animation_frame(c.as_ref().unchecked_ref());
        }
    }) as Box<dyn FnMut(f64)>));

    let _ = web_sys::window().unwrap().request_animation_frame(
        closure.borrow().as_ref().unwrap().as_ref().unchecked_ref()
    );
}

#[cfg(feature = "hydrate")]
fn new_particle(w: f64, h: f64, scheme: u8) -> Particle {
    let rnd = js_sys::Math::random;
    let hue = match scheme {
        0 => 180.0 + rnd() * 120.0,
        1 => rnd() * 60.0,
        2 => 170.0 + rnd() * 70.0,
        _ => rnd() * 360.0,
    };
    Particle {
        x: rnd() * w, y: rnd() * h,
        vx: (rnd() - 0.5) * 1.8, vy: (rnd() - 0.5) * 1.8,
        hue, size: 1.5 + rnd() * 2.5,
    }
}
