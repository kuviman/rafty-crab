use super::*;

pub struct ModelDraw {
    geng: Geng,
    assets: Rc<Assets>,
    quad: ugli::VertexBuffer<Vertex>,
}

#[derive(ugli::Vertex)]
struct Vertex {
    a_pos: vec3<f32>,
    a_uv: vec2<f32>,
}

impl ModelDraw {
    pub fn new(geng: &Geng, assets: &Rc<Assets>) -> Self {
        Self {
            geng: geng.clone(),
            assets: assets.clone(),
            quad: ugli::VertexBuffer::new_static(
                geng.ugli(),
                vec![
                    Vertex {
                        a_pos: vec3(0.0, 0.0, 0.0),
                        a_uv: vec2(0.0, 0.0),
                    },
                    Vertex {
                        a_pos: vec3(0.0, 1.0, 0.0),
                        a_uv: vec2(0.0, 1.0),
                    },
                    Vertex {
                        a_pos: vec3(1.0, 1.0, 0.0),
                        a_uv: vec2(1.0, 1.0),
                    },
                    Vertex {
                        a_pos: vec3(1.0, 0.0, 0.0),
                        a_uv: vec2(1.0, 0.0),
                    },
                ],
            ),
        }
    }
    pub fn draw(
        &self,
        framebuffer: &mut ugli::Framebuffer,
        camera: &impl AbstractCamera3d,
        model: &pog_paint::Model,
        transform: mat4<f32>,
    ) {
        let framebuffer_size = framebuffer.size().map(|x| x as f32);
        for plane in &model.planes {
            if let Some(texture) = &plane.texture.texture {
                let model_matrix = transform
                    * plane.transform
                    * mat4::translate(plane.texture.offset.map(|x| x as f32).extend(0.0))
                    * mat4::scale(texture.size().map(|x| x as f32).extend(1.0));
                ugli::draw(
                    framebuffer,
                    &self.assets.shaders.model,
                    ugli::DrawMode::TriangleFan,
                    &self.quad,
                    (
                        ugli::uniforms! {
                            u_texture: texture,
                            u_model_matrix: model_matrix,
                        },
                        camera.uniforms(framebuffer_size),
                    ),
                    ugli::DrawParameters {
                        depth_func: Some(ugli::DepthFunc::LessOrEqual),
                        blend_mode: None,
                        ..default()
                    },
                );
            }
        }
    }
}
