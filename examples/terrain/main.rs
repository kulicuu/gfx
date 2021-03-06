// Copyright 2014 The Gfx-rs Developers.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

extern crate cgmath;
#[macro_use]
extern crate gfx;
extern crate gfx_app;
extern crate genmesh;
extern crate noise;
extern crate winit;

use gfx::format::{DepthStencil};
use gfx::GraphicsPoolExt;
use gfx_app::{BackbufferView, ColorFormat, DepthFormat};

use cgmath::{Deg, Matrix4, Point3, SquareMatrix, Vector3};
use genmesh::{Vertices, Triangulate};
use genmesh::generators::{Plane, SharedVertex, IndexedPolygon};
use noise::{NoiseModule, Perlin};
use std::time::{Instant};

gfx_defines!{
    vertex Vertex {
        pos: [f32; 3] = "a_Pos",
        color: [f32; 3] = "a_Color",
    }

    constant Locals {
        model: [[f32; 4]; 4] = "u_Model",
        view: [[f32; 4]; 4] = "u_View",
        proj: [[f32; 4]; 4] = "u_Proj",
    }

    pipeline pipe {
        vbuf: gfx::VertexBuffer<Vertex> = (),
        locals: gfx::ConstantBuffer<Locals> = "Locals",
        model: gfx::Global<[[f32; 4]; 4]> = "u_Model",
        view: gfx::Global<[[f32; 4]; 4]> = "u_View",
        proj: gfx::Global<[[f32; 4]; 4]> = "u_Proj",
        out_color: gfx::RenderTarget<ColorFormat> = "Target0",
        out_depth: gfx::DepthTarget<DepthFormat> =
            gfx::preset::depth::LESS_EQUAL_WRITE,
    }
}

fn calculate_color(height: f32) -> [f32; 3] {
    if height > 8.0 {
        [0.9, 0.9, 0.9] // white
    } else if height > 0.0 {
        [0.7, 0.7, 0.7] // greay
    } else if height > -5.0 {
        [0.2, 0.7, 0.2] // green
    } else {
        [0.2, 0.2, 0.7] // blue
    }
}

struct App<B: gfx::Backend> {
    views: Vec<BackbufferView<B::Resources>>,
    pso: gfx::PipelineState<B::Resources, pipe::Meta>,
    data: pipe::Data<B::Resources>,
    slice: gfx::Slice<B::Resources>,
    start_time: Instant,
}

impl<B: gfx::Backend> gfx_app::Application<B> for App<B> {
    fn new(factory: &mut B::Factory,
           _: &mut gfx::queue::GraphicsQueue<B>,
           backend: gfx_app::shade::Backend,
           window_targets: gfx_app::WindowTargets<B::Resources>) -> Self
    {
        use gfx::traits::FactoryExt;

        let vs = gfx_app::shade::Source {
            glsl_120: include_bytes!("shader/terrain_120.glslv"),
            glsl_150: include_bytes!("shader/terrain_150.glslv"),
            hlsl_40:  include_bytes!("data/vertex.fx"),
            msl_11: include_bytes!("shader/terrain_vertex.metal"),
            vulkan:   include_bytes!("data/vert.spv"),
            .. gfx_app::shade::Source::empty()
        };
        let ps = gfx_app::shade::Source {
            glsl_120: include_bytes!("shader/terrain_120.glslf"),
            glsl_150: include_bytes!("shader/terrain_150.glslf"),
            hlsl_40:  include_bytes!("data/pixel.fx"),
            msl_11: include_bytes!("shader/terrain_frag.metal"),
            vulkan:   include_bytes!("data/frag.spv"),
            .. gfx_app::shade::Source::empty()
        };

        let perlin = Perlin::new();
        let plane = Plane::subdivide(256, 256);
        let vertex_data: Vec<Vertex> = plane.shared_vertex_iter()
            .map(|genmesh::Vertex { pos, .. }| {
                let (x, y) = (pos[0], pos[1]);
                let h = perlin.get([x, y]) * 32.0;
                Vertex {
                    pos: [25.0 * x, 25.0 * y, h],
                    color: calculate_color(h),
                }
            })
            .collect();

        let index_data: Vec<u32> = plane.indexed_polygon_iter()
            .triangulate()
            .vertices()
            .map(|i| i as u32)
            .collect();

        let (vbuf, slice) = factory.create_vertex_buffer_with_slice(&vertex_data, &index_data[..]);
        let (out_color, out_depth) = window_targets.views[0].clone(); // initial placeholder values

        App {
            views: window_targets.views,
            pso: factory.create_pipeline_simple(
                vs.select(backend).unwrap(),
                ps.select(backend).unwrap(),
                pipe::new()
                ).unwrap(),
            data: pipe::Data {
                vbuf,
                locals: factory.create_constant_buffer(1),
                model: Matrix4::identity().into(),
                view: Matrix4::identity().into(),
                proj: cgmath::perspective(
                    Deg(60.0f32), window_targets.aspect_ratio, 0.1, 1000.0
                    ).into(),
                out_color,
                out_depth,
            },
            slice,
            start_time: Instant::now(),
        }
    }

    fn render(&mut self, (frame, sync): (gfx::Frame, &gfx_app::SyncPrimitives<B::Resources>),
              pool: &mut gfx::GraphicsCommandPool<B>, queue: &mut gfx::queue::GraphicsQueue<B>)
    {
        let elapsed = self.start_time.elapsed();
        let time = elapsed.as_secs() as f32 + elapsed.subsec_nanos() as f32 / 1000_000_000.0;
        let x = time.sin();
        let y = time.cos();
        let view = Matrix4::look_at(
            Point3::new(x * 32.0, y * 32.0, 16.0),
            Point3::new(0.0, 0.0, 0.0),
            Vector3::unit_z(),
        );

        let (cur_color, cur_depth) = self.views[frame.id()].clone();
        self.data.out_color = cur_color;
        self.data.out_depth = cur_depth;
        self.data.view = view.into();
        let locals = Locals {
            model: self.data.model,
            view: self.data.view,
            proj: self.data.proj,
        };

        let mut encoder = pool.acquire_graphics_encoder();
        encoder.update_buffer(&self.data.locals, &[locals], 0).unwrap();
        encoder.clear(&self.data.out_color, [0.3, 0.3, 0.3, 1.0]);
        encoder.clear_depth(&self.data.out_depth, 1.0);
        encoder.draw(&self.slice, &self.pso, &self.data);
        encoder.synced_flush(queue, &[&sync.rendering], &[], Some(&sync.frame_fence));
    }

    fn on_resize(&mut self, window_targets: gfx_app::WindowTargets<B::Resources>) {
        self.views = window_targets.views;
        self.data.proj = cgmath::perspective(
                Deg(60.0f32), window_targets.aspect_ratio, 0.1, 1000.0
            ).into();
    }
}

pub fn main() {
    use gfx_app::Application;
    App::launch_simple("Terrain example");
}
