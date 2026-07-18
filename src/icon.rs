use tray_icon::Icon;

const SIZE: u32 = 32;

pub fn make_icon(remaining: Option<u8>) -> Result<Icon, String> {
    Icon::from_rgba(rgba_icon(remaining), SIZE, SIZE)
        .map_err(|error| format!("could not create tray icon: {error}"))
}

fn rgba_icon(remaining: Option<u8>) -> Vec<u8> {
    let progress = remaining.unwrap_or(0).min(100) as f32 / 100.0;
    let active = status_color(remaining);
    let inactive = [110, 118, 128];
    let mut rgba = vec![0; (SIZE * SIZE * 4) as usize];
    let center = (SIZE as f32 - 1.0) / 2.0;

    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let radius = (dx * dx + dy * dy).sqrt();
            let edge_alpha = ring_alpha(radius);
            if edge_alpha <= 0.0 {
                continue;
            }

            let mut angle = dx.atan2(-dy) / std::f32::consts::TAU;
            if angle < 0.0 {
                angle += 1.0;
            }
            let color = if remaining.is_some() && angle <= progress {
                active
            } else {
                inactive
            };
            let offset = ((y * SIZE + x) * 4) as usize;
            rgba[offset] = color[0];
            rgba[offset + 1] = color[1];
            rgba[offset + 2] = color[2];
            rgba[offset + 3] = (edge_alpha * 255.0) as u8;
        }
    }
    rgba
}

fn ring_alpha(radius: f32) -> f32 {
    let outer = 14.5;
    let inner = 9.0;
    let outer_fade = (outer + 0.5 - radius).clamp(0.0, 1.0);
    let inner_fade = (radius - inner + 0.5).clamp(0.0, 1.0);
    outer_fade.min(inner_fade)
}

fn status_color(remaining: Option<u8>) -> [u8; 3] {
    match remaining {
        Some(0..=14) => [220, 65, 74],
        Some(15..=39) => [224, 154, 42],
        Some(_) => [45, 167, 92],
        None => [110, 118, 128],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_has_valid_dimensions_and_visible_pixels() {
        let rgba = rgba_icon(Some(50));
        assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
        assert!(rgba.chunks_exact(4).any(|pixel| pixel[3] > 0));
    }

    #[test]
    fn thresholds_have_distinct_colors() {
        assert_ne!(status_color(Some(10)), status_color(Some(20)));
        assert_ne!(status_color(Some(20)), status_color(Some(80)));
    }
}
