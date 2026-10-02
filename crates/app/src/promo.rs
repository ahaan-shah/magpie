//! The launch film: a keynote-style ~34 s promo of the real app.
//!
//! The app runs headlessly on a virtual clock (Paper theme, demo data),
//! driven by a scripted cursor, and renders straight into a GPU texture
//! (4K, with mipmaps so it stays sharp at any size). A second egui context
//! composites the stage in 1080p with 4x MSAA: a deep navy backdrop with
//! soft light, the app as a floating window that takes the stage for each
//! feature and then glides aside for a big headline, kinetic type, a few
//! vector birds, and a keynote-style sign-off. Only the final frame leaves
//! the GPU, on its way to NVENC.
//!
//! ```text
//! cargo test --profile fast -p magpie-finance promo -- --ignored --nocapture
//! MAGPIE_PROMO_STILLS=4.5,10.2 …   # just those moments, as PNGs (fast)
//! ```
//! Writes `~/Videos/magpie/magpie-promo.mp4` (override with MAGPIE_PROMO_OUT).

use crate::app::{App, Page, logo};
use crate::film::{Act, Beat, Driver, Target, encoder_args, gpu_setup, mark, paint_cursor};
use crate::motion::{ease_in_out, ease_out, ease_out_back};
use crate::theme;
use egui::epaint::{Mesh, Shadow};
use egui::{Align2, Color32, Context, CornerRadius, Id, Key, Modifiers, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use egui_wgpu::wgpu;
use std::f32::consts::{PI, TAU};
use std::io::Write;
use std::process::{Command, Stdio};

const AW: f32 = 1600.0;
const AH: f32 = 900.0;
/// The app renders at 3840x2160 so close-ups stay crisp.
const APP_PPP: f32 = 2.4;
const OW: f32 = 1920.0;
const OH: f32 = 1080.0;
const FPS: f32 = 60.0;
const DURATION: f32 = 34.0;
const MIPS: u32 = 5;
const PERSPECTIVE: f32 = 2600.0;

const WHITE: Color32 = Color32::WHITE;
const ORANGE: Color32 = Color32::from_rgb(0xF2, 0x9A, 0x62);
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

// ================================================================ GPU stage

const MIP_SHADER: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct V { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn vs(@builtin(vertex_index) i: u32) -> V {
    var p = array<vec2<f32>, 3>(vec2(-1.0, -1.0), vec2(3.0, -1.0), vec2(-1.0, 3.0));
    var o: V;
    o.pos = vec4(p[i], 0.0, 1.0);
    o.uv = vec2((p[i].x + 1.0) * 0.5, 1.0 - (p[i].y + 1.0) * 0.5);
    return o;
}
@fragment fn fs(v: V) -> @location(0) vec4<f32> { return textureSample(src, samp, v.uv); }
"#;

/// Two egui renderers on one GPU: the app draws into a mipmapped texture,
/// the stage samples it and draws the frame, which is read back for encoding.
struct Stage {
    device: wgpu::Device,
    queue: wgpu::Queue,
    app: egui_wgpu::Renderer,
    comp: egui_wgpu::Renderer,
    app_levels: Vec<wgpu::TextureView>,
    mip_pipe: wgpu::RenderPipeline,
    mip_groups: Vec<wgpu::BindGroup>,
    app_id: egui::TextureId,
    msaa: wgpu::TextureView,
    out: wgpu::Texture,
    out_view: wgpu::TextureView,
    readback: wgpu::Buffer,
}

impl Stage {
    fn new() -> Stage {
        let rs = egui_kittest::wgpu::create_render_state(gpu_setup(), egui_wgpu::RendererOptions::default());
        let device = rs.device.clone();
        let queue = rs.queue.clone();
        let opts = |msaa: u32| egui_wgpu::RendererOptions {
            msaa_samples: msaa,
            depth_stencil_format: None,
            dithering: true,
            predictable_texture_filtering: false,
        };
        let app = egui_wgpu::Renderer::new(&device, FORMAT, opts(1));
        let mut comp = egui_wgpu::Renderer::new(&device, FORMAT, opts(4));
        let (w, h) = ((AW * APP_PPP) as u32, (AH * APP_PPP) as u32);
        let app_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("app"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: MIPS,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let app_levels: Vec<wgpu::TextureView> = (0..MIPS)
            .map(|l| {
                app_tex.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: l,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            })
            .collect();
        let full = app_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let app_id = comp.register_native_texture_with_sampler_options(
            &device,
            &full,
            wgpu::SamplerDescriptor {
                label: Some("app"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                ..Default::default()
            },
        );
        // Mipmaps: each level is a linear-filtered half of the one above.
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mips"),
            source: wgpu::ShaderSource::Wgsl(MIP_SHADER.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mips"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mips"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let mip_pipe = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mips"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mip_groups = (1..MIPS as usize)
            .map(|l| {
                device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("mip"),
                    layout: &bgl,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&app_levels[l - 1]),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&sampler),
                        },
                    ],
                })
            })
            .collect();
        let size = wgpu::Extent3d {
            width: OW as u32,
            height: OH as u32,
            depth_or_array_layers: 1,
        };
        let msaa = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("msaa"),
                size,
                mip_level_count: 1,
                sample_count: 4,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        let out = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("out"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let out_view = out.create_view(&wgpu::TextureViewDescriptor::default());
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (OW as u64) * 4 * (OH as u64),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Stage {
            device,
            queue,
            app,
            comp,
            app_levels,
            mip_pipe,
            mip_groups,
            app_id,
            msaa,
            out,
            out_view,
            readback,
        }
    }

    /// Tessellates and draws one egui frame into `view`.
    #[allow(clippy::too_many_arguments)]
    fn draw(
        renderer: &mut egui_wgpu::Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        ctx: &Context,
        out: &mut egui::FullOutput,
        view: &wgpu::TextureView,
        resolve: Option<&wgpu::TextureView>,
        px: [u32; 2],
        clear: wgpu::Color,
    ) {
        for (id, deltas) in out.textures_delta.set.drain() {
            for delta in deltas {
                renderer.update_texture(device, queue, id, &delta);
            }
        }
        let jobs = ctx.tessellate(std::mem::take(&mut out.shapes), out.pixels_per_point);
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: px,
            pixels_per_point: out.pixels_per_point,
        };
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let bufs = renderer.update_buffers(device, queue, &mut enc, &jobs, &screen);
        {
            let mut pass = enc
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("egui"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        depth_slice: None,
                        resolve_target: resolve,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(clear),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                })
                .forget_lifetime();
            renderer.render(&mut pass, &jobs, &screen);
        }
        queue.submit(bufs.into_iter().chain(std::iter::once(enc.finish())));
        for id in out.textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
    }

    /// Applies texture changes (fonts and such) for a frame that isn't drawn.
    fn update_app_textures(&mut self, out: &mut egui::FullOutput) {
        for (id, deltas) in out.textures_delta.set.drain() {
            for delta in deltas {
                self.app.update_texture(&self.device, &self.queue, id, &delta);
            }
        }
        for id in out.textures_delta.free.drain() {
            self.app.free_texture(&id);
        }
    }

    fn render_app(&mut self, ctx: &Context, out: &mut egui::FullOutput) {
        let px = [(AW * APP_PPP) as u32, (AH * APP_PPP) as u32];
        Self::draw(
            &mut self.app,
            &self.device,
            &self.queue,
            ctx,
            out,
            &self.app_levels[0],
            None,
            px,
            wgpu::Color::WHITE,
        );
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        for l in 1..MIPS as usize {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mip"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.app_levels[l],
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&self.mip_pipe);
            pass.set_bind_group(0, &self.mip_groups[l - 1], &[]);
            pass.draw(0..3, 0..1);
        }
        self.queue.submit(std::iter::once(enc.finish()));
    }

    fn render_comp(&mut self, ctx: &Context, out: &mut egui::FullOutput, frame: &mut [u8]) {
        Self::draw(
            &mut self.comp,
            &self.device,
            &self.queue,
            ctx,
            out,
            &self.msaa,
            Some(&self.out_view),
            [OW as u32, OH as u32],
            wgpu::Color::BLACK,
        );
        let mut enc = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        enc.copy_texture_to_buffer(
            self.out.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &self.readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(OW as u32 * 4),
                    rows_per_image: None,
                },
            },
            wgpu::Extent3d {
                width: OW as u32,
                height: OH as u32,
                depth_or_array_layers: 1,
            },
        );
        let idx = self.queue.submit(std::iter::once(enc.finish()));
        let slice = self.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| drop(tx.send(r)));
        self.device
            .poll(wgpu::PollType::Wait {
                submission_index: Some(idx),
                timeout: Some(std::time::Duration::from_secs(10)),
            })
            .expect("poll");
        rx.recv().expect("map").expect("map ok");
        frame.copy_from_slice(&slice.get_mapped_range().expect("mapped"));
        self.readback.unmap();
    }
}

// ================================================================ the script

#[derive(Clone, Copy)]
enum Focus {
    Center,
    At(Pos2),
}

/// Where the app window sits: `focus` (an app point) appears at `at` (a
/// screen point), `scale` screen px per app pt, turned `tilt` radians about
/// its vertical axis.
#[derive(Clone, Copy)]
struct Pose {
    focus: Pos2,
    at: Pos2,
    scale: f32,
    tilt: f32,
    alpha: f32,
}

impl Pose {
    fn project(&self, p: Pos2) -> Pos2 {
        let c_app = pos2(AW / 2.0, AH / 2.0);
        let center = self.at + (c_app - self.focus) * self.scale;
        let v = (p - c_app) * self.scale;
        let (s, c) = self.tilt.sin_cos();
        let k = PERSPECTIVE / (PERSPECTIVE + v.x * s);
        center + vec2(v.x * c, v.y) * k
    }
}

struct Shot {
    t: f32,
    focus: Focus,
    target: Option<Target>,
    at: Pos2,
    scale: f32,
    tilt: f32,
    alpha: f32,
}

fn key(t: f32, at: (f32, f32), scale: f32, tilt: f32, alpha: f32) -> Shot {
    Shot {
        t,
        focus: Focus::Center,
        target: None,
        at: pos2(at.0, at.1),
        scale,
        tilt,
        alpha,
    }
}

fn key_on(t: f32, target: Target, offset: Vec2, at: (f32, f32), scale: f32) -> Shot {
    Shot {
        t,
        focus: Focus::At(pos2(offset.x, offset.y)),
        target: Some(target),
        at: pos2(at.0, at.1),
        scale,
        tilt: 0.0,
        alpha: 1.0,
    }
}

enum Hook {
    Go(Page),
    Theme(&'static str),
    Import,
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Left,
    Right,
    Top,
    Center,
}

struct Text {
    t0: f32,
    t1: f32,
    side: Side,
    lines: &'static [&'static str],
    sub: &'static str,
    size: f32,
}

struct Script {
    beats: Vec<Beat>,
    keys: Vec<Shot>,
    hooks: Vec<(f32, Hook)>,
    texts: Vec<Text>,
}

/// Window positions between features.
const FULL: (f32, f32) = (960.0, 548.0);
const RIGHT: (f32, f32) = (1335.0, 540.0);
const LEFT: (f32, f32) = (585.0, 540.0);
const ASIDE: f32 = 0.56;
const TILT: f32 = 0.2;
const END: f32 = 30.4;

fn script() -> Script {
    let b = |t: f32, act: Act| Beat { t, act };
    let mv = |t: f32, target: Target, off: Vec2, dur: f32| Beat {
        t,
        act: Act::Move(target, off, dur),
    };
    let quick = Target::Widget(Id::new("quick-add"));
    let hero = mark("dash:hero");
    let rep = mark("rep:cash");
    let beats = vec![
        // Dashboard: glide across the net worth chart.
        b(3.0, Act::Move(Target::Point(pos2(1180.0, 330.0)), Vec2::ZERO, 0.01)),
        mv(4.1, hero.clone(), vec2(-120.0, 40.0), 1.0),
        mv(5.0, hero.clone(), vec2(260.0, 20.0), 0.9),
        // Quick add (the page changed while the window was aside).
        mv(8.2, quick.clone(), vec2(-220.0, 0.0), 0.6),
        b(8.9, Act::Click),
        b(9.1, Act::Type("coffee 4.50 @Blue Bottle #treats", 16.0)),
        b(11.5, Act::Key(Key::Enter, Modifiers::NONE)),
        // Import: the modal opened while aside; click Import.
        mv(14.6, mark("btn:Import"), Vec2::ZERO, 0.9),
        b(15.8, Act::Click),
        // Reports.
        mv(19.4, rep.clone(), vec2(-300.0, 20.0), 0.6),
        mv(20.1, rep.clone(), vec2(0.0, 0.0), 0.8),
        mv(21.0, rep.clone(), vec2(260.0, 10.0), 0.7),
        mv(23.0, Target::Point(pos2(AW / 2.0, AH - 60.0)), Vec2::ZERO, 0.6),
    ];
    let keys = vec![
        key(0.0, (960.0, 1560.0), 0.86, 0.0, 0.0),
        key(2.1, (960.0, 1560.0), 0.86, 0.0, 0.0),
        key(3.1, FULL, 1.0, 0.0, 1.0),
        key(3.4, FULL, 1.0, 0.0, 1.0),
        key_on(4.2, hero.clone(), Vec2::ZERO, (960.0, 520.0), 1.6),
        key_on(5.7, hero, Vec2::ZERO, (960.0, 520.0), 1.6),
        key(6.5, RIGHT, ASIDE, -TILT, 1.0),
        key(7.8, RIGHT, ASIDE, -TILT, 1.0),
        key_on(8.6, quick.clone(), vec2(-140.0, 60.0), (960.0, 520.0), 1.75),
        key_on(11.9, quick, vec2(-140.0, 60.0), (960.0, 520.0), 1.75),
        key(12.7, LEFT, ASIDE, TILT, 1.0),
        key(13.9, LEFT, ASIDE, TILT, 1.0),
        key(14.7, (960.0, 560.0), 1.3, 0.0, 1.0),
        key(16.4, (960.0, 560.0), 1.3, 0.0, 1.0),
        key(17.2, RIGHT, ASIDE, -TILT, 1.0),
        key(18.5, RIGHT, ASIDE, -TILT, 1.0),
        key_on(19.3, rep.clone(), Vec2::ZERO, (960.0, 540.0), 1.5),
        key_on(21.6, rep, Vec2::ZERO, (960.0, 540.0), 1.5),
        key(22.4, LEFT, ASIDE, TILT, 1.0),
        key(23.6, LEFT, ASIDE, TILT, 1.0),
        key(24.4, (960.0, 640.0), 0.66, 0.0, 1.0),
        key(27.7, (960.0, 640.0), 0.66, 0.0, 1.0),
        key(28.5, (960.0, 760.0), 0.5, 0.0, 0.0),
        key(DURATION, (960.0, 760.0), 0.5, 0.0, 0.0),
    ];
    let hooks = vec![
        (7.65, Hook::Go(Page::Ledger)),
        (13.75, Hook::Import),
        (18.35, Hook::Go(Page::Reports)),
        (23.45, Hook::Go(Page::Dashboard)),
        (25.2, Hook::Theme("Midnight")),
        (25.9, Hook::Theme("Tokyo Night")),
        (26.6, Hook::Theme("Rosé Pine Dawn")),
        (27.3, Hook::Theme("Paper")),
    ];
    let texts = vec![
        Text {
            t0: 0.25,
            t1: 2.4,
            side: Side::Center,
            lines: &["Your money.", "Finally *calm*."],
            sub: "",
            size: 132.0,
        },
        Text {
            t0: 6.2,
            t1: 8.1,
            side: Side::Left,
            lines: &["Every dollar,", "one *calm* view."],
            sub: "Net worth, cash flow and budgets, together on one beautiful dashboard.",
            size: 84.0,
        },
        Text {
            t0: 12.4,
            t1: 14.3,
            side: Side::Right,
            lines: &["Just *type* it.", "Magpie gets it."],
            sub: "Amounts, payees, tags and dates, understood the moment you write them.",
            size: 84.0,
        },
        Text {
            t0: 16.9,
            t1: 18.9,
            side: Side::Left,
            lines: &["Your bank,", "*imported*."],
            sub: "Drop in any statement: CSV, Excel or OFX. Magpie maps the columns for you.",
            size: 84.0,
        },
        Text {
            t0: 22.1,
            t1: 24.0,
            side: Side::Right,
            lines: &["See where", "it all *goes*."],
            sub: "Clear reports for this month, this year, or any range you choose.",
            size: 84.0,
        },
        Text {
            t0: 24.3,
            t1: 27.9,
            side: Side::Top,
            lines: &["Make it *yours*."],
            sub: "14 themes. 9 fonts. One calm app.",
            size: 76.0,
        },
        Text {
            t0: 28.3,
            t1: 30.1,
            side: Side::Center,
            lines: &["Private by *design*."],
            sub: "Your data stays on your computer. No account. No cloud. No tracking.",
            size: 96.0,
        },
    ];
    Script {
        beats,
        keys,
        hooks,
        texts,
    }
}

/// The window's pose at `t`. Between two keys, the next key's focus point
/// travels in a straight line to its spot while the scale changes
/// geometrically, so a zoom never swings past its target.
fn pose_at(keys: &[Shot], resolved: &[Pos2], t: f32) -> Pose {
    let pose = |i: usize| Pose {
        focus: resolved[i],
        at: keys[i].at,
        scale: keys[i].scale,
        tilt: keys[i].tilt,
        alpha: keys[i].alpha,
    };
    let i = keys.iter().rposition(|k| k.t <= t).unwrap_or(0);
    let j = (i + 1).min(keys.len() - 1);
    let (a, b) = (pose(i), pose(j));
    if j == i || keys[j].t <= keys[i].t {
        return a;
    }
    let k = ease_in_out(((t - keys[i].t) / (keys[j].t - keys[i].t)).clamp(0.0, 1.0));
    let start = a.project(b.focus);
    let want = start.lerp(b.at, k);
    let mut p = Pose {
        focus: b.focus,
        at: want,
        scale: a.scale * (b.scale / a.scale).powf(k),
        tilt: a.tilt + (b.tilt - a.tilt) * k,
        alpha: a.alpha + (b.alpha - a.alpha) * k,
    };
    // With a tilt, `at` isn't exactly where the focus lands; correct for it.
    p.at += want - p.project(p.focus);
    p
}

// ================================================================ painting

/// A soft radial light with a smooth falloff (no visible rings).
fn soft_glow(p: &egui::Painter, c: Pos2, radius: f32, color: Color32) {
    let rings = 64;
    let segs = 96;
    let [r, g, b, a] = color.to_srgba_unmultiplied();
    let at = |k: f32| {
        let f = (-3.5 * k * k).exp() * (1.0 - k).max(0.0).sqrt();
        Color32::from_rgba_unmultiplied(r, g, b, (a as f32 * f) as u8)
    };
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, at(0.0));
    for ring in 1..=rings {
        let k = ring as f32 / rings as f32;
        for s in 0..segs {
            let ang = TAU * s as f32 / segs as f32;
            mesh.colored_vertex(c + vec2(ang.cos(), ang.sin()) * radius * k, at(k));
        }
    }
    for s in 0..segs as u32 {
        mesh.add_triangle(0, 1 + s, 1 + (s + 1) % segs as u32);
    }
    for ring in 1..rings as u32 {
        let base = 1 + (ring - 1) * segs as u32;
        let next = base + segs as u32;
        for s in 0..segs as u32 {
            let n = (s + 1) % segs as u32;
            mesh.add_triangle(base + s, next + s, next + n);
            mesh.add_triangle(base + s, next + n, base + n);
        }
    }
    p.add(Shape::mesh(mesh));
}

fn backdrop(p: &egui::Painter, t: f32, window: Pos2, window_alpha: f32) {
    let full = Rect::from_min_size(Pos2::ZERO, vec2(OW, OH));
    let top = Color32::from_rgb(0x0B, 0x10, 0x1F);
    let bottom = Color32::from_rgb(0x07, 0x0A, 0x14);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(full.left_top(), top);
    mesh.colored_vertex(full.right_top(), top);
    mesh.colored_vertex(full.right_bottom(), bottom);
    mesh.colored_vertex(full.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(Shape::mesh(mesh));
    let drift = |ph: f32| vec2((t * 0.13 + ph).sin(), (t * 0.1 + ph * 1.7).cos()) * 60.0;
    soft_glow(
        p,
        pos2(960.0, 520.0),
        1150.0,
        Color32::from_rgba_unmultiplied(0x1C, 0x27, 0x48, 255),
    );
    soft_glow(
        p,
        pos2(260.0, 120.0) + drift(0.0),
        900.0,
        Color32::from_rgba_unmultiplied(0x4F, 0x5E, 0xD8, 70),
    );
    soft_glow(
        p,
        pos2(1700.0, 980.0) + drift(2.0),
        850.0,
        Color32::from_rgba_unmultiplied(0x1F, 0x9E, 0x8E, 55),
    );
    // Warm light that follows the app around the stage.
    soft_glow(
        p,
        window + drift(4.0) * 0.4,
        820.0,
        Color32::from_rgba_unmultiplied(0xE4, 0x81, 0x4F, (85.0 * window_alpha) as u8),
    );
}

/// The app as a floating window: a fine textured grid (so the tilt has
/// true perspective) with its corners rounded off.
fn paint_window(p: &egui::Painter, pose: &Pose, tex: egui::TextureId) {
    if pose.alpha <= 0.002 {
        return;
    }
    const R: f32 = 16.0;
    let (nx, ny) = (48u32, 27u32);
    let corner = |q: Pos2| -> Pos2 {
        let cx = if q.x < R {
            R
        } else if q.x > AW - R {
            AW - R
        } else {
            return q;
        };
        let cy = if q.y < R {
            R
        } else if q.y > AH - R {
            AH - R
        } else {
            return q;
        };
        let c = pos2(cx, cy);
        let d = q - c;
        if d.length() > R { c + d.normalized() * R } else { q }
    };
    let tint = WHITE.gamma_multiply(pose.alpha);
    let mut mesh = Mesh::with_texture(tex);
    for j in 0..=ny {
        for i in 0..=nx {
            let a = pos2(AW * i as f32 / nx as f32, AH * j as f32 / ny as f32);
            mesh.vertices.push(egui::epaint::Vertex {
                pos: pose.project(corner(a)),
                uv: pos2(a.x / AW, a.y / AH),
                color: tint,
            });
        }
    }
    let w = nx + 1;
    for j in 0..ny {
        for i in 0..nx {
            let v = j * w + i;
            mesh.add_triangle(v, v + 1, v + w + 1);
            mesh.add_triangle(v, v + w + 1, v + w);
        }
    }
    let outline: Vec<Pos2> = (0..72)
        .map(|i| {
            let k = (i % 18) as f32 / 17.0;
            let (c, a0) = match i / 18 {
                0 => (pos2(AW - R, R), -PI / 2.0),
                1 => (pos2(AW - R, AH - R), 0.0),
                2 => (pos2(R, AH - R), PI / 2.0),
                _ => (pos2(R, R), PI),
            };
            let ang = a0 + k * PI / 2.0;
            pose.project(c + vec2(ang.cos(), ang.sin()) * R)
        })
        .collect();
    let bounds = Rect::from_points(&outline);
    p.add(
        Shadow {
            offset: [0, (40.0 * pose.scale).min(120.0) as i8],
            blur: (110.0 * pose.scale).min(255.0) as u8,
            spread: 0,
            color: Color32::from_black_alpha((150.0 * pose.alpha) as u8),
        }
        .as_shape(
            bounds.shrink(20.0 * pose.scale),
            CornerRadius::same((R * pose.scale).min(250.0) as u8),
        ),
    );
    p.add(Shape::mesh(mesh));
    p.add(Shape::closed_line(
        outline,
        Stroke::new(1.2, Color32::from_white_alpha((34.0 * pose.alpha) as u8)),
    ));
}

/// One headline line, with `*accent*` words in orange.
fn line_job(text: &str, font: &egui::FontId, alpha: f32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    for (i, part) in text.split('*').enumerate() {
        if part.is_empty() {
            continue;
        }
        let color = if i % 2 == 1 { ORANGE } else { WHITE };
        job.append(
            part,
            0.0,
            egui::TextFormat {
                font_id: font.clone(),
                color: color.gamma_multiply(alpha),
                ..Default::default()
            },
        );
    }
    job
}

/// Kinetic type: each line rises out of a mask, staggered, then the block
/// lifts away.
fn paint_text(p: &egui::Painter, t: f32, x: &Text) {
    if t < x.t0 || t > x.t1 + 0.5 {
        return;
    }
    let font = theme::display(x.size);
    let lh = x.size * 1.1;
    let sub_size = if x.side == Side::Center { 32.0 } else { 30.0 };
    let wrap = match x.side {
        Side::Left | Side::Right => 640.0,
        _ => 1200.0,
    };
    let sub_h = if x.sub.is_empty() {
        0.0
    } else {
        28.0 + p
            .layout(x.sub.to_string(), theme::regular(sub_size), WHITE, wrap)
            .size()
            .y
    };
    let block_h = lh * x.lines.len() as f32 + sub_h;
    let (left, centered, top) = match x.side {
        Side::Left => (150.0, false, OH / 2.0 - block_h / 2.0),
        Side::Right => (1130.0, false, OH / 2.0 - block_h / 2.0),
        Side::Top => (0.0, true, 86.0),
        Side::Center => (0.0, true, OH / 2.0 - block_h / 2.0),
    };
    let out_k = ease_in_out(((t - x.t1) / 0.45).clamp(0.0, 1.0));
    let lift = -out_k * 40.0;
    let fade = 1.0 - out_k;
    for (i, line) in x.lines.iter().enumerate() {
        let k = ease_out(((t - x.t0 - i as f32 * 0.09) / 0.75).clamp(0.0, 1.0));
        let alpha = k * fade;
        if alpha <= 0.0 {
            continue;
        }
        let g = p.layout_job(line_job(line, &font, alpha));
        let y = top + i as f32 * lh;
        let xpos = if centered { OW / 2.0 - g.size().x / 2.0 } else { left };
        let mask = Rect::from_min_size(pos2(0.0, y - 12.0 + lift), vec2(OW, lh + 18.0));
        p.with_clip_rect(mask)
            .galley(pos2(xpos, y + (1.0 - k) * lh * 0.9 + lift), g, WHITE);
    }
    if !x.sub.is_empty() {
        let k = ease_out(((t - x.t0 - 0.3) / 0.8).clamp(0.0, 1.0));
        let g = p.layout(
            x.sub.to_string(),
            theme::regular(sub_size),
            Color32::from_white_alpha((178.0 * k * fade) as u8),
            wrap,
        );
        let y = top + lh * x.lines.len() as f32 + 28.0 + (1.0 - k) * 14.0 + lift;
        let xpos = if centered { OW / 2.0 - g.size().x / 2.0 } else { left };
        p.galley(pos2(xpos, y), g, WHITE);
    }
}

/// A minimalist bird: two curved strokes, wings beating.
fn paint_bird(p: &egui::Painter, c: Pos2, span: f32, flap: f32, alpha: f32) {
    let pts: Vec<Pos2> = (0..=16)
        .map(|i| {
            let u = i as f32 / 8.0 - 1.0;
            let s = u.abs();
            let lift = flap * 0.42 * s.powf(1.4) + 0.16 * (PI * s).sin();
            c + vec2(u * span, -lift * span)
        })
        .collect();
    p.add(Shape::line(
        pts,
        Stroke::new(2.4, Color32::from_white_alpha((150.0 * alpha) as u8)),
    ));
}

/// Small flocks that glide by now and then: (start, duration, from, to, size).
type Flock = (f32, f32, (f32, f32), (f32, f32), f32);
const FLOCKS: &[Flock] = &[
    (0.2, 7.0, (-200.0, 240.0), (2100.0, 120.0), 26.0),
    (12.4, 7.5, (2100.0, 120.0), (-200.0, 70.0), 18.0),
    (30.2, 6.5, (-200.0, 190.0), (2100.0, 110.0), 24.0),
];

fn birds(p: &egui::Painter, t: f32) {
    let formation = [(0.0, 0.0, 1.0), (-70.0, 34.0, 0.8), (-120.0, -18.0, 0.7)];
    for (n, &(t0, dur, from, to, size)) in FLOCKS.iter().enumerate() {
        let k = (t - t0) / dur;
        if !(0.0..=1.0).contains(&k) {
            continue;
        }
        let dir = if to.0 > from.0 { 1.0 } else { -1.0 };
        let base = pos2(from.0, from.1).lerp(pos2(to.0, to.1), k);
        let fade = (k / 0.08).min((1.0 - k) / 0.08).clamp(0.0, 1.0);
        for (i, &(dx, dy, s)) in formation.iter().enumerate() {
            let ph = (n * 3 + i) as f32 * 1.3;
            let beat = ((t * 0.7 + ph).sin() * 0.5 + 0.5).powf(1.5); // flap, then glide
            let flap = (t * 5.0 + ph).sin() * (0.15 + 0.85 * beat);
            let bob = (t * 1.1 + ph).sin() * 6.0;
            paint_bird(
                p,
                base + vec2(dx * dir, dy + bob),
                size * s,
                flap,
                fade * (0.55 + 0.45 * s),
            );
        }
    }
}

/// The keynote sign-off; `t` counts from its start.
fn end_card(p: &egui::Painter, t: f32) {
    let k = |d: f32, len: f32| ease_out(((t - d) / len).clamp(0.0, 1.0));
    let c = pos2(OW / 2.0, 400.0);
    let pop = ease_out_back((t / 0.8).clamp(0.0, 1.0));
    if pop > 0.0 {
        soft_glow(
            p,
            c,
            440.0,
            Color32::from_rgba_unmultiplied(0xE4, 0x81, 0x4F, (95.0 * k(0.0, 0.8)) as u8),
        );
        let size = 150.0 * (0.75 + 0.25 * pop);
        let mut lp = p.clone();
        lp.set_opacity(k(0.0, 0.5));
        logo(
            &lp,
            Rect::from_center_size(c, Vec2::splat(size)),
            &theme::Theme::by_name("Paper"),
        );
    }
    let k1 = k(0.35, 0.7);
    p.text(
        pos2(OW / 2.0, 580.0 + (1.0 - k1) * 18.0),
        Align2::CENTER_CENTER,
        "Magpie",
        theme::display(104.0),
        WHITE.gamma_multiply(k1),
    );
    let k2 = k(0.65, 0.7);
    let mut job = egui::text::LayoutJob::default();
    job.append(
        "A calm nest for your money.",
        0.0,
        egui::TextFormat {
            font_id: theme::regular(34.0),
            color: Color32::from_white_alpha((190.0 * k2) as u8),
            italics: true,
            ..Default::default()
        },
    );
    let g = p.layout_job(job);
    p.galley(pos2(OW / 2.0 - g.size().x / 2.0, 646.0 + (1.0 - k2) * 12.0), g, WHITE);
    let k3 = k(1.0, 0.7);
    if k3 > 0.0 {
        let label = p.layout_no_wrap("Download free".into(), theme::semibold(28.0), WHITE.gamma_multiply(k3));
        let r = Rect::from_center_size(
            pos2(OW / 2.0, 770.0 + (1.0 - k3) * 14.0),
            vec2(label.size().x + 84.0, 70.0),
        );
        p.add(
            Shadow {
                offset: [0, 14],
                blur: 40,
                spread: 0,
                color: Color32::from_rgba_unmultiplied(0xE4, 0x81, 0x4F, (90.0 * k3) as u8),
            }
            .as_shape(r, CornerRadius::same(35)),
        );
        p.rect_filled(
            r,
            CornerRadius::same(35),
            Color32::from_rgb(0xD9, 0x6C, 0x37).gamma_multiply(k3),
        );
        p.galley(r.center() - label.size() / 2.0, label, WHITE);
    }
    let k4 = k(1.3, 0.7);
    p.text(
        pos2(OW / 2.0, 856.0),
        Align2::CENTER_CENTER,
        "Linux & macOS  ·  Free & open source  ·  github.com/ahaan-shah/magpie",
        theme::medium(22.0),
        Color32::from_white_alpha((150.0 * k4) as u8),
    );
}

// ================================================================ the film

/// A short, realistic bank statement for the import scene.
fn statement_csv() -> std::path::PathBuf {
    let path = std::env::temp_dir().join("magpie-promo-statement.csv");
    let rows = [
        ("28/09/2026", "WHOLE FOODS MARKET", "86.42", ""),
        ("27/09/2026", "UBER *TRIP", "18.90", ""),
        ("27/09/2026", "SPOTIFY", "11.99", ""),
        ("26/09/2026", "SWEETGREEN", "15.75", ""),
        ("25/09/2026", "ACME PAYROLL", "", "4,250.00"),
        ("25/09/2026", "TRADER JOE'S", "54.18", ""),
        ("24/09/2026", "NETFLIX.COM", "15.49", ""),
        ("23/09/2026", "SHELL OIL", "48.20", ""),
        ("22/09/2026", "AMAZON MKTPLACE", "32.60", ""),
        ("21/09/2026", "BLUE BOTTLE COFFEE", "6.25", ""),
    ];
    let mut s = String::from("Date,Description,Debit,Credit\n");
    for (d, n, debit, credit) in rows {
        s.push_str(&format!("{d},{n},\"{debit}\",\"{credit}\"\n"));
    }
    std::fs::write(&path, s).expect("write statement");
    path
}

fn raw(size: Vec2, ppp: f32, t: f32, events: Vec<egui::Event>) -> egui::RawInput {
    let mut raw = egui::RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(t as f64),
        events,
        ..Default::default()
    };
    raw.viewports.insert(
        egui::ViewportId::ROOT,
        egui::ViewportInfo {
            native_pixels_per_point: Some(ppp),
            ..Default::default()
        },
    );
    raw
}

/// Runs a fresh, deterministic copy of the app through the script, calling
/// `each` after every frame. It runs twice: once to learn where things
/// are, once to film.
fn simulate(script: &Script, mut each: impl FnMut(f32, &Context, &mut egui::FullOutput, &Driver)) {
    let dir = std::env::temp_dir().join("magpie-promo");
    let _ = std::fs::remove_dir_all(&dir);
    let mut store = magpie_core::Store::open(&dir).expect("store");
    magpie_core::demo::generate(&mut store, magpie_core::Cur::USD, 24, 0).expect("demo");
    store.update_settings(|s| s.theme = "Paper".into()).expect("theme");
    let ctx = Context::default();
    let mut app = App::with_context(&ctx, None, store);
    let csv = statement_csv();
    let beats = script
        .beats
        .iter()
        .map(|b| Beat {
            t: b.t,
            act: b.act.clone(),
        })
        .collect();
    let mut driver = Driver::new(beats, pos2(AW / 2.0, AH / 2.0));
    let mut next_hook = 0;
    let mut started = false;
    for frame in 0..(DURATION * FPS) as usize {
        let t = frame as f32 / FPS;
        while next_hook < script.hooks.len() && script.hooks[next_hook].0 <= t {
            match &script.hooks[next_hook].1 {
                Hook::Go(page) => app.go(&ctx, *page),
                Hook::Theme(name) => app.set_theme(&ctx, name),
                Hook::Import => {
                    let form = crate::forms::ImportForm::open(&app.store, csv.clone()).expect("statement");
                    app.open_modal(&ctx, crate::forms::Modal::Import(Box::new(form)));
                }
            }
            next_hook += 1;
        }
        let events = driver.events(&ctx, t);
        let on = t >= 2.4;
        if on && !started {
            started = true;
            app.shown_at = t as f64;
        }
        let input = raw(vec2(AW, AH), APP_PPP, t, if on { events } else { Vec::new() });
        let mut out = ctx.run_ui(input, |ui| app.frame(ui));
        each(t, &ctx, &mut out, &driver);
    }
}

#[test]
#[ignore = "renders the launch promo (slow); run explicitly"]
fn promo() {
    // SAFETY: single-threaded test setup before the app reads these.
    unsafe {
        std::env::set_var("MAGPIE_HEADLESS", "1");
        std::env::set_var("MAGPIE_TODAY", "2026-09-29");
    }
    crate::marks::enable();
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let out_path =
        std::env::var("MAGPIE_PROMO_OUT").unwrap_or_else(|_| format!("{home}/Videos/magpie/magpie-promo.mp4"));
    let stills: Option<Vec<f32>> = std::env::var("MAGPIE_PROMO_STILLS")
        .ok()
        .map(|s| s.split(',').filter_map(|x| x.trim().parse().ok()).collect());
    let out_dir = std::path::Path::new(&out_path).parent().expect("out dir").to_path_buf();
    std::fs::create_dir_all(&out_dir).expect("out dir");
    let script = script();

    // Pass 1: learn where each camera target is at its key's time.
    let mut resolved: Vec<Option<Pos2>> = vec![None; script.keys.len()];
    simulate(&script, |t, ctx, _out, _driver| {
        for (i, k) in script.keys.iter().enumerate() {
            if resolved[i].is_none() && k.t <= t + 0.5 / FPS {
                resolved[i] = Some(match (&k.target, k.focus) {
                    (Some(target), Focus::At(off)) => target
                        .rect(ctx)
                        .map(|r| r.center() + off.to_vec2())
                        .unwrap_or(pos2(AW / 2.0, AH / 2.0)),
                    (None, Focus::At(p)) => p,
                    (_, Focus::Center) => pos2(AW / 2.0, AH / 2.0),
                });
            }
        }
    });
    let resolved: Vec<Pos2> = resolved
        .into_iter()
        .map(|p| p.unwrap_or(pos2(AW / 2.0, AH / 2.0)))
        .collect();

    // Pass 2: film.
    let mut stage = Stage::new();
    let comp = Context::default();
    theme::install_fonts(&comp, "Inter");
    let mut ffmpeg = stills.is_none().then(|| {
        Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgba"])
            .args([
                "-s",
                &format!("{}x{}", OW as u32, OH as u32),
                "-r",
                &format!("{FPS}"),
                "-i",
                "-",
            ])
            .args(encoder_args())
            .args(["-movflags", "+faststart", &out_path])
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg")
    });
    let mut sink = ffmpeg.as_mut().map(|f| f.stdin.take().expect("stdin"));
    let mut frame = vec![0u8; OW as usize * OH as usize * 4];
    let clock = std::time::Instant::now();
    let mut n = 0usize;
    simulate(&script, |t, ctx, out, driver| {
        n += 1;
        let wanted = match &stills {
            Some(list) => list.iter().any(|s| (s - t).abs() < 0.5 / FPS),
            None => true,
        };
        let pose = pose_at(&script.keys, &resolved, t);
        if !wanted || pose.alpha <= 0.0 {
            stage.update_app_textures(out);
            if !wanted {
                return;
            }
        } else {
            stage.render_app(ctx, out);
        }
        let app_id = stage.app_id;
        let mut comp_out = comp.run_ui(raw(vec2(OW, OH), 1.0, t, Vec::new()), |ui| {
            let p = ui.painter().clone();
            backdrop(&p, t, pose.project(pos2(AW / 2.0, AH / 2.0)), pose.alpha);
            birds(&p, t);
            paint_window(&p, &pose, app_id);
            // The cursor only shows while the app has the stage.
            let cursor = ((pose.scale - 0.85) / 0.2).clamp(0.0, 1.0) * pose.alpha;
            if cursor > 0.0 && t > 3.2 {
                let s = 1.2 * (pose.scale / 1.2).sqrt();
                for (at, t0) in &driver.pointer.ripples {
                    let k = (t - t0) / 0.55;
                    if (0.0..1.0).contains(&k) {
                        let c = pose.project(*at);
                        let rr = (10.0 + 30.0 * ease_out(k)) * s;
                        p.circle_filled(c, rr, ORANGE.gamma_multiply(0.22 * (1.0 - k) * cursor));
                        p.circle_stroke(
                            c,
                            rr,
                            Stroke::new(2.0 * s, ORANGE.gamma_multiply(0.7 * (1.0 - k) * cursor)),
                        );
                    }
                }
                let mut cp = p.clone();
                cp.set_opacity(cursor);
                paint_cursor(&cp, pose.project(driver.pointer.pos), s);
            }
            for x in &script.texts {
                paint_text(&p, t, x);
            }
            if t > END {
                end_card(&p, t - END);
            }
        });
        stage.render_comp(&comp, &mut comp_out, &mut frame);
        match &mut sink {
            Some(s) => s.write_all(&frame).expect("write frame"),
            None => {
                let path = out_dir.join(format!("promo-{t:05.2}.png"));
                if let Some(png) = image::RgbaImage::from_raw(OW as u32, OH as u32, frame.clone()) {
                    png.save(&path).expect("save");
                    eprintln!("promo: wrote {}", path.display());
                }
            }
        }
        if n.is_multiple_of(300) {
            eprintln!("promo: {t:>4.1}s / {DURATION}s  ({:.0?})", clock.elapsed());
        }
    });
    drop(sink);
    if let Some(mut f) = ffmpeg {
        assert!(f.wait().expect("ffmpeg").success(), "ffmpeg failed");
        eprintln!("promo: wrote {out_path} in {:.0?}", clock.elapsed());
    }
}
