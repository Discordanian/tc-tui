//! ASCII rotating globe inspired by the encom-globe aesthetic (Tron: Legacy boardroom scene).
//! Renders an Earth-like shaded sphere with orthographic projection.

/// Characters for water shading (dark to bright)
const WATER_CHARS: &[char] = &[' ', '.', ':', '~', '~'];
/// Characters for land shading
const LAND_CHARS: &[char] = &['.', ':', ';', '*', 'o', 'O', '#', '@'];

/// Returns true if (lat_deg, lon_deg) is approximately over land.
/// Uses simplified continental outlines (rough rectangles).
fn is_land(lat_deg: f64, lon_deg: f64) -> bool {
    // North America
    if lat_deg >= 15.0 && lat_deg <= 72.0 && lon_deg >= -168.0 && lon_deg <= -50.0 {
        return true;
    }
    // South America
    if lat_deg >= -55.0 && lat_deg <= 12.0 && lon_deg >= -82.0 && lon_deg <= -35.0 {
        return true;
    }
    // Greenland
    if lat_deg >= 60.0 && lat_deg <= 83.0 && lon_deg >= -50.0 && lon_deg <= -20.0 {
        return true;
    }
    // Europe
    if lat_deg >= 36.0 && lat_deg <= 71.0 && lon_deg >= -10.0 && lon_deg <= 40.0 {
        return true;
    }
    // Africa
    if lat_deg >= -35.0 && lat_deg <= 37.0 && lon_deg >= -18.0 && lon_deg <= 52.0 {
        return true;
    }
    // Asia
    if lat_deg >= 10.0 && lat_deg <= 75.0 && lon_deg >= 40.0 && lon_deg <= 180.0 {
        return true;
    }
    // Southeast Asia / Indonesia
    if lat_deg >= -10.0 && lat_deg <= 25.0 && lon_deg >= 95.0 && lon_deg <= 145.0 {
        return true;
    }
    // Australia
    if lat_deg >= -39.0 && lat_deg <= -10.0 && lon_deg >= 113.0 && lon_deg <= 154.0 {
        return true;
    }
    // New Zealand
    if lat_deg >= -47.0 && lat_deg <= -34.0 && lon_deg >= 166.0 && lon_deg <= 179.0 {
        return true;
    }
    // Madagascar
    if lat_deg >= -26.0 && lat_deg <= -12.0 && lon_deg >= 43.0 && lon_deg <= 50.0 {
        return true;
    }
    // UK
    if lat_deg >= 50.0 && lat_deg <= 60.0 && lon_deg >= -8.0 && lon_deg <= 2.0 {
        return true;
    }
    // Japan
    if lat_deg >= 31.0 && lat_deg <= 45.0 && lon_deg >= 129.0 && lon_deg <= 146.0 {
        return true;
    }
    false
}

/// Renders a rotating ASCII globe into a string.
/// Resolution scales with `width` and `height` - larger panels show more detail.
/// `angle` is the rotation angle in radians (increases over time for spin).
pub fn render_globe(width: usize, height: usize, angle: f64) -> String {
    let mut buffer = vec![vec![' '; width]; height];

    let cx = width as f64 / 2.0;
    let cy = height as f64 / 2.0;
    let radius = (width.min(height) as f64 / 2.0) * 0.9;

    // Light direction (top-right-front for shading)
    let light_x: f64 = 0.5;
    let light_y: f64 = -0.5;
    let light_z: f64 = 0.7;
    let light_len = (light_x * light_x + light_y * light_y + light_z * light_z).sqrt();

    let cos_a = angle.cos();
    let sin_a = angle.sin();

    // Scale sampling with display size for better resolution on larger terminals
    let steps = (width.min(height) * 2).max(40).min(120);
    for i in 0..=steps {
        for j in 0..=steps {
            let theta = std::f64::consts::PI * (i as f64 / steps as f64);
            let phi = std::f64::consts::TAU * (j as f64 / steps as f64);

            // Sphere: x = sin(theta)*cos(phi), y = cos(theta), z = sin(theta)*sin(phi)
            let x = theta.sin() * phi.cos();
            let y = theta.cos();
            let z = theta.sin() * phi.sin();

            // Rotate around Y axis
            let x_rot = x * cos_a - z * sin_a;
            let z_rot = x * sin_a + z * cos_a;

            // Only front-facing
            if z_rot <= 0.0 {
                continue;
            }

            // Orthographic projection
            let screen_x = (cx + x_rot * radius) as usize;
            let screen_y = (cy + y * radius) as usize;

            if screen_x < width && screen_y < height {
                // Convert to lat/lon for Earth texture (theta=0 at north pole)
                let lat_deg = 90.0 - theta * 180.0 / std::f64::consts::PI;
                let lon_deg = phi * 180.0 / std::f64::consts::PI - 180.0;

                let land = is_land(lat_deg, lon_deg);

                // Surface normal (same as position for unit sphere) for shading
                let dot = (x_rot * light_x + y * light_y + z_rot * light_z) / light_len;
                let shade = ((dot + 1.0) / 2.0).clamp(0.0, 1.0);

                let chars = if land { LAND_CHARS } else { WATER_CHARS };
                let idx = (shade * (chars.len() - 1) as f64) as usize;
                buffer[screen_y][screen_x] = chars[idx];
            }
        }
    }

    buffer
        .iter()
        .map(|row| row.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
