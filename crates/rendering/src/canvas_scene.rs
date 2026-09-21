use crate::{GAME_ATLAS_SIDE, GameFrame, web::Sprite};
use wasm_bindgen::JsValue;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

pub(super) fn draw_scene_sprite(
    context: &CanvasRenderingContext2d,
    atlas: &HtmlCanvasElement,
    canvas: &HtmlCanvasElement,
    sprite: (Sprite, GameFrame),
    selected: bool,
) -> Result<(), String> {
    let (sprite, frame) = sprite;
    let width = f64::from(canvas.width());
    let height = f64::from(canvas.height());
    let (x, y) = sprite_canvas_origin(sprite, [width, height], frame.size);
    let [mut sx, sy, sw, sh] = sprite
        .uv
        .map(|value| f64::from(value) * f64::from(GAME_ATLAS_SIDE));
    context.save();
    let result = if sw < 0.0 {
        sx += sw;
        context
            .translate(x + f64::from(frame.size[0]), y)
            .map_err(error)?;
        context.scale(-1.0, 1.0).map_err(error)?;
        context
            .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                atlas,
                sx,
                sy,
                sw.abs(),
                sh,
                0.0,
                0.0,
                f64::from(frame.size[0]),
                f64::from(frame.size[1]),
            )
            .map_err(error)
    } else {
        context
            .draw_image_with_html_canvas_element_and_sw_and_sh_and_dx_and_dy_and_dw_and_dh(
                atlas,
                sx,
                sy,
                sw,
                sh,
                x,
                y,
                f64::from(frame.size[0]),
                f64::from(frame.size[1]),
            )
            .map_err(error)
    };
    context.restore();
    result?;
    if selected {
        context.begin_path();
        context.set_stroke_style_str("#f2dc78");
        context
            .ellipse(
                x + f64::from(frame.size[0]) / 2.0,
                y + f64::from(frame.size[1]),
                f64::from(frame.size[0]) * 0.55,
                f64::from(frame.size[1]) * 0.105,
                0.0,
                0.0,
                std::f64::consts::TAU,
            )
            .map_err(error)?;
        context.stroke();
    }
    Ok(())
}

fn error(e: impl Into<JsValue>) -> String {
    format!("Canvas rendering unavailable: {:?}", e.into())
}

fn sprite_canvas_origin(sprite: Sprite, canvas: [f64; 2], size: [f32; 2]) -> (f64, f64) {
    (
        (f64::from(sprite.position[0]) + 1.0) * canvas[0] / 2.0 - f64::from(size[0]) / 2.0,
        (1.0 - f64::from(sprite.position[1])) * canvas[1] / 2.0 - f64::from(size[1]) / 2.0,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    wasm_bindgen_test_configure!(run_in_browser);

    #[wasm_bindgen_test]
    fn canvas_origin_uses_clip_rect_center_for_off_center_anchor() {
        let screen = [50.0_f64, 40.0_f64];
        let anchor = [5.0_f32, 30.0_f32];
        let size = [20.0_f32, 40.0_f32];
        let top_left = [
            screen[0] - f64::from(anchor[0]),
            screen[1] - f64::from(anchor[1]),
        ];
        let sprite = Sprite {
            position: [
                ((top_left[0] + f64::from(size[0]) / 2.0) / 50.0 - 1.0) as f32,
                (1.0 - (top_left[1] + f64::from(size[1]) / 2.0) / 40.0) as f32,
            ],
            radius: [size[0] / 100.0, size[1] / 80.0],
            color: [1.0; 4],
            uv: [0.0, 0.0, 0.1, 0.2],
        };
        let (x, y) = sprite_canvas_origin(sprite, [100.0, 80.0], size);
        assert!((x - top_left[0]).abs() < 1e-6);
        assert!((y - top_left[1]).abs() < 1e-6);
    }
}
