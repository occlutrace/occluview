//! GPU texture upload: decodes a CPU-side [`MeshTexture`] into a `wgpu::Texture`
//! + view + sampler + bind group, ready to bind at group 2.

use crate::pipeline::Renderer;
use occluview_core::MeshTexture;

/// A texture resident on the GPU: the `wgpu::Texture`, its view, a sampler,
/// and the bind group (group 2) that binds them at bindings 0 and 1.
pub struct GpuTexture {
    /// Owns the GPU memory; kept alive so the view and sampler stay valid.
    #[allow(dead_code)]
    pub(crate) texture: wgpu::Texture,
    /// Kept alive so the bind group's view binding remains valid.
    #[allow(dead_code)]
    pub(crate) view: wgpu::TextureView,
    /// Kept alive so the bind group's sampler binding remains valid.
    #[allow(dead_code)]
    pub(crate) sampler: wgpu::Sampler,
    /// Bind group (group 2): binding 0 = view, binding 1 = sampler.
    pub bind_group: wgpu::BindGroup,
}

impl GpuTexture {
    /// Upload a CPU-side [`MeshTexture`] to the GPU and build the group-2 bind
    /// group. Uses linear filtering and clamp-to-edge wrapping — the sane
    /// defaults for dental mesh textures.
    #[must_use]
    pub fn upload(
        renderer: &Renderer,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        tex: &MeshTexture,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("occluview mesh texture"),
            size: wgpu::Extent3d {
                width: tex.width,
                height: tex.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &tex.rgba,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(tex.width * 4),
                rows_per_image: Some(tex.height),
            },
            wgpu::Extent3d {
                width: tex.width,
                height: tex.height,
                depth_or_array_layers: 1,
            },
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("occluview mesh sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("occluview mesh texture bind group"),
            layout: renderer.texture_layout(),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        Self {
            texture,
            view,
            sampler,
            bind_group,
        }
    }
}
