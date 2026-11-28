//! Copies an editor's picture into its effect slot: one textured quad over the slot's rect, inside
//! Rune's pass, under the scissor Rune set to the slot's clip.

use rune::render::EffectSite;

/// egui-wgpu draws into a non-sRGB target, so it writes sRGB-encoded colours (its "gamma
/// framebuffer" path). The quad decodes them when the frame blends in linear.
const SHADER: &str = r"
struct Place {
    rect: vec4<f32>,
    decode: vec4<f32>,
};
@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var nearest: sampler;
@group(0) @binding(2) var<uniform> place: Place;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> Out {
    let uv = vec2<f32>(f32(i & 1u), f32(i >> 1u));
    var out: Out;
    out.pos = vec4<f32>(mix(place.rect.xy, place.rect.zw, uv), 0.0, 1.0);
    out.uv = uv;
    return out;
}

fn linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3<f32>(2.4)), c / 12.92, c <= vec3<f32>(0.04045));
}

@fragment
fn fs(in: Out) -> @location(0) vec4<f32> {
    let c = textureSample(picture, nearest, in.uv);
    if place.decode.x > 0.5 && c.a > 0.0 {
        return vec4<f32>(linear(c.rgb / c.a) * c.a, c.a);
    }
    return c;
}
";

pub struct Blit {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    decode: bool,
}

/// What [`Blit::picture`] makes for one editor.
pub struct Target {
    pub view: wgpu::TextureView,
    pub bind: wgpu::BindGroup,
    pub place: wgpu::Buffer,
}

impl Blit {
    pub const SOURCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

    /// `srgb_blend`: the frame's colours are sRGB-encoded (`Frame::srgb_blend`), so the quad
    /// copies the picture as it is; otherwise it decodes it to linear.
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, srgb_blend: bool) -> Blit {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas editor blit"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let entry = |binding, ty| wgpu::BindGroupLayoutEntry { binding, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty, count: None };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas editor blit"),
            entries: &[
                entry(0, wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                }),
                entry(1, wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)),
                entry(2, wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas editor blit"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("canvas editor blit"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleStrip, ..Default::default() },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("canvas editor blit"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        Blit { pipeline, layout, sampler, decode: !srgb_blend }
    }

    /// A picture texture of `size` device pixels and what draws it.
    pub fn picture(&self, device: &wgpu::Device, size: [u32; 2]) -> Target {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("canvas editor picture"),
            size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: Self::SOURCE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let place = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("canvas editor place"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("canvas editor picture"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: place.as_entire_binding() },
            ],
        });
        Target { view, bind, place }
    }

    /// Draws `target` over the slot `site` describes.
    pub fn draw(&self, queue: &wgpu::Queue, pass: &mut wgpu::RenderPass<'_>, site: &EffectSite<'_>, target: &Target) {
        let [x, y, w, h] = site.instance.rect;
        let (ox, oy) = (site.origin[0], site.origin[1]);
        let (tw, th) = (site.size[0].max(1) as f32, site.size[1].max(1) as f32);
        let ndc = |px: f32, py: f32| [(px - ox) / tw * 2.0 - 1.0, 1.0 - (py - oy) / th * 2.0];
        let [l, t] = ndc(x, y);
        let [r, b] = ndc(x + w, y + h);
        let decode = if self.decode { 1.0f32 } else { 0.0 };
        let words = [l, t, r, b, decode, 0.0, 0.0, 0.0];
        let bytes: Vec<u8> = words.iter().flat_map(|v| v.to_le_bytes()).collect();
        queue.write_buffer(&target.place, 0, &bytes);
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &target.bind, &[]);
        pass.draw(0..4, 0..1);
    }
}
