use vek::*;
use derive_more::{Add, Mul};
use euc::{Pipeline, Buffer2d, Target, PixelMode, DepthMode, TriangleList, Empty, Linear, Texture, Sampler, AaMode, Unit, Clamped};
use minifb::{Key, MouseButton, MouseMode, Window, WindowOptions};
use std::marker::PhantomData;

struct TeapotShadow<'a> {
    mvp: Mat4<f32>,
    phantom: PhantomData<&'a ()>,
}

impl<'a> Pipeline for TeapotShadow<'a> {
    type Vertex = wavefront::Vertex<'a>;
    type VertexData = f32;
    type Primitives = TriangleList;
    type Fragment = Unit;
    type Pixel = ();

    fn pixel_mode(&self) -> PixelMode { PixelMode::PASS }
    fn depth_mode(&self) -> DepthMode { DepthMode::LESS_WRITE }
    fn rasterizer_config(&self) -> euc::CullMode { euc::CullMode::None }

    #[inline(always)]
    fn vertex_shader(&self, vertex: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        ((self.mvp * Vec4::from_point(Vec3::from(vertex.position()))).into_array(), 0.0)
    }

    #[inline(always)]
    fn fragment_shader(&self, _: Self::VertexData) -> Self::Fragment { Unit }

    #[inline(always)]
    fn blend_shader(&self, old: Self::Pixel, new: Self::Fragment) {}
}

struct Teapot<'a> {
    m: Mat4<f32>,
    v: Mat4<f32>,
    p: Mat4<f32>,
    light_pos: Vec3<f32>,
    shadow: Clamped<Linear<&'a Buffer2d<f32>>>,
    light_vp: Mat4<f32>,
}

#[derive(Add, Mul, Clone)]
struct VertexData {
    wpos: Vec3<f32>,
    wnorm: Vec3<f32>,
    light_view_pos: Vec3<f32>,
}

impl<'a> Pipeline for Teapot<'a> {
    type Vertex = wavefront::Vertex<'a>;
    type VertexData = VertexData;
    type Primitives = TriangleList;
    type Fragment = Rgba<f32>;
    type Pixel = u32;

    fn depth_mode(&self) -> DepthMode { DepthMode::LESS_WRITE }
    fn aa_mode(&self) -> AaMode { AaMode::Msaa { level: 1 } }

    #[inline(always)]
    fn vertex_shader(&self, vertex: &Self::Vertex) -> ([f32; 4], Self::VertexData) {
        let wpos = self.m * Vec4::from_point(Vec3::from(vertex.position()));
        let wnorm = self.m * Vec4::from_direction(-Vec3::from(vertex.normal().unwrap()));

        let light_view_pos = self.light_vp * Vec4::from_point(wpos);
        let light_view_pos = light_view_pos.xyz() / light_view_pos.w;
        (
            (self.p * self.v * wpos).into_array(),
            VertexData { wpos: wpos.xyz(), wnorm: wnorm.xyz(), light_view_pos },
        )
    }

    #[inline(always)]
    fn fragment_shader(&self, VertexData { wpos, wnorm, light_view_pos }: Self::VertexData) -> Self::Fragment {
        let wnorm = wnorm.normalized();
        let cam_pos = Vec3::zero();
        let cam_dir = (wpos - cam_pos).normalized();
        let light_dir = (wpos - self.light_pos).normalized();
        let surf_color = Rgba::new(1.0, 0.8, 0.7, 1.0);

        // Phong reflection model
        let ambient = 0.1;
        let diffuse = wnorm.dot(-light_dir).max(0.0) * 0.5;
        let specular = (-light_dir).reflected(wnorm).dot(-cam_dir).max(0.0).powf(30.0) * 3.0;

        // Shadow-mapping
        let light_depth = self.shadow.sample((light_view_pos.xy() * Vec2::new(1.0, -1.0) * 0.5 + 0.5).into_array()) + 0.0001;
        let depth = light_view_pos.z;
        let in_light = depth < light_depth;

        let light = ambient + if in_light { diffuse + specular } else { 0.0 };
        surf_color * light
    }

    #[inline(always)]
    fn blend_shader(&self, _old: Self::Pixel, rgba: Self::Fragment) -> Self::Pixel {
        let rgba = rgba.map(|e| e.clamped(0.0, 1.0) * 255.0).as_();
        // The window's framebuffer uses BGRA format
        let bgra = Rgba::new(rgba.b, rgba.g, rgba.r, rgba.a);
        u32::from_le_bytes(bgra.into_array())
    }
}

fn main() {
    let [w, h] = [1280, 960];

    let mut color = Buffer2d::fill([w, h], 0x0);
    let mut depth = Buffer2d::fill([w, h], 1.0);
    let mut shadow = Buffer2d::fill([512; 2], 1.0);

    let model = wavefront::Obj::from_file("examples/data/teapot.obj").unwrap();

    let mut win = Window::new("Teapot", w, h, WindowOptions::default()).unwrap();

    let mut ori = Vec2::new(0.0, 0.0);
    let mut dist = 6.0;
    let mut old_mouse_pos = (0.0, 0.0);

    let mut i = 0;
    while win.is_open() && !win.is_key_down(Key::Escape) {
        let start_time = std::time::Instant::now();

        // Clear the render targets ready for the next frame
        color.clear(0x0);
        depth.clear(1.0);
        shadow.clear(1.0);

        // Update camera as the mouse moves
        let mouse_pos = win.get_mouse_pos(MouseMode::Pass).unwrap_or_default();
        if win.get_mouse_down(MouseButton::Left) {
            ori.x -= (mouse_pos.1 - old_mouse_pos.1) * 0.003;
            ori.y += (mouse_pos.0 - old_mouse_pos.0) * 0.003;
        }
        if win.get_mouse_down(MouseButton::Right) {
            dist = (dist + (mouse_pos.1 - old_mouse_pos.1) as f32 * 0.01).max(1.0).min(20.0);
        }
        old_mouse_pos = mouse_pos;

        // Position of objects in the scene
        let teapot_pos = Vec3::new(0.0, 0.0, 0.0);
        let light_pos = Vec3::<f32>::new(-8.0, 5.0, -5.0);

        // Set up the light matrix
        let light_p = Mat4::perspective_fov_lh_zo(0.75, shadow.size()[0] as f32, shadow.size()[1] as f32, 0.1, 100.0);
        let light_v = Mat4::look_at_lh(light_pos, -teapot_pos, Vec3::unit_y());
        let light_vp = light_p * light_v;

        // Set up the camera matrix
        let p = Mat4::perspective_fov_lh_zo(1.3, w as f32, h as f32, 0.01, 100.0);
        let v = Mat4::<f32>::identity()
            * Mat4::translation_3d(Vec3::new(0.0, 0.0, dist));
        // Set up the teapot matrix
        let m = Mat4::<f32>::translation_3d(-teapot_pos)
            * Mat4::rotation_x(core::f32::consts::PI)
            * Mat4::rotation_x(ori.x)
            * Mat4::rotation_y(ori.y);

        // Shadow pass
        TeapotShadow { mvp: light_vp * m, phantom: PhantomData }.render(
            model.vertices(),
            &mut Empty::default(),
            &mut shadow,
        );

        // Colour pass
        Teapot { m, v, p, light_pos, shadow: Clamped::new(Linear::new(&shadow)), light_vp }.render(
            model.vertices(),
            &mut color,
            &mut depth,
        );

        win.update_with_buffer(color.raw(), w, h).unwrap();

        if i % 60 == 0 {
            let elapsed = start_time.elapsed();
            win.set_title(&format!("Teapot (Time = {:?}, FPS = {})", elapsed, 1.0 / elapsed.as_secs_f32()));
        }
        i += 1;
    }
}
