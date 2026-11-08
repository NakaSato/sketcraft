//! Original SketCraft mark, shared by desktop, packaging and CLI: a slab with a block pulled up
//! out of it (push/pull), drawn borderless as six faces with transparent gaps between them. Same
//! faces and gradients as the macOS icon's
//! `macos/source/SketCraft.icon/Assets/03-solid-blue-cutout.svg`.
type P = (f32, f32);

/// One face in that SVG's units: its outline, and its linear gradient from one point and
/// colour to another.
struct Face {
    outline: &'static [P],
    from: (P, [u8; 3]),
    to: (P, [u8; 3]),
}

const FACES: [Face; 6] = [
    // Top of the slab, around the block.
    Face {
        outline: &[(296.546, 522.551), (135.092, 615.767), (512.0, 833.374), (888.908, 615.767), (727.454, 522.551), (512.0, 646.944)],
        from: ((398.928, 522.551), [69, 159, 255]),
        to: ((625.072, 833.374), [35, 121, 231]),
    },
    // Right side of the slab.
    Face {
        outline: &[(906.908, 740.159), (530.0, 957.767), (530.0, 864.551), (906.908, 646.944)],
        from: ((661.918, 646.944), [27, 102, 207]),
        to: ((774.99, 957.767), [18, 69, 150]),
    },
    // Left side of the slab.
    Face {
        outline: &[(117.092, 740.159), (494.0, 957.767), (494.0, 864.551), (117.092, 646.944)],
        from: ((249.01, 646.944), [37, 141, 245]),
        to: ((362.082, 957.767), [20, 97, 218]),
    },
    // Right side of the pulled block.
    Face {
        outline: &[(709.454, 491.374), (530.0, 594.982), (530.0, 294.551), (709.454, 190.944)],
        from: ((592.809, 190.944), [27, 102, 207]),
        to: ((646.645, 594.982), [18, 69, 150]),
    },
    // Left side of the pulled block.
    Face {
        outline: &[(314.546, 491.374), (494.0, 594.982), (494.0, 294.551), (314.546, 190.944)],
        from: ((377.355, 190.944), [37, 141, 245]),
        to: ((431.191, 594.982), [20, 97, 218]),
    },
    // Top of the pulled block.
    Face {
        outline: &[(512.0, 56.159), (691.454, 159.767), (512.0, 263.374), (332.546, 159.767)],
        from: ((458.164, 56.159), [81, 173, 255]),
        to: ((565.836, 263.374), [43, 131, 241]),
    },
];

/// The mark's extent in SVG units: left, top, width, height.
const BOUNDS: (f32, f32, f32, f32) = (117.092, 56.159, 789.816, 901.608);

/// This block is the icon image data
pub fn rgba(size: usize) -> Result<Vec<u8>, String> {
    if !(1..=2048).contains(&size) {
        return Err("icon size must be 1..=2048".into());
    }
    // Coverage buffer at 4x4 samples per pixel (2x2 for big sizes), averaged down at the end.
    let ss = if size > 512 { 2 } else { 4 };
    let n = size * ss;
    let mut buf = vec![[0u8; 4]; n * n];
    let (bx, by, bw, bh) = BOUNDS;
    let s = n as f32 / bh; // SVG units -> samples: the mark fills the height
    let (ox, oy) = ((n as f32 - bw * s) / 2.0 - bx * s, -by * s); // and is centred sideways
    // Fill each face's outline (even-odd spans per row, so concave outlines work) with its gradient.
    for face in &FACES {
        let v: Vec<P> = face.outline.iter().map(|p| (ox + p.0 * s, oy + p.1 * s)).collect();
        let (((fx, fy), c0), ((tx, ty), c1)) = (face.from, face.to);
        let (gx, gy) = (tx - fx, ty - fy);
        let g2 = gx * gx + gy * gy;
        let (top, bottom) = v.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        let mut xs = Vec::with_capacity(v.len());
        for y in ((top - 0.5).ceil() as i32).max(0)..=((bottom - 0.5).floor() as i32).min(n as i32 - 1) {
            let cy = y as f32 + 0.5;
            xs.clear();
            for i in 0..v.len() {
                let (a, b) = (v[i], v[(i + 1) % v.len()]);
                if (a.1 <= cy) != (b.1 <= cy) {
                    xs.push(a.0 + (cy - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(f32::total_cmp);
            for &[from, to] in xs.as_chunks::<2>().0 {
                for x in ((from - 0.5).ceil() as i32).max(0)..=((to - 0.5).floor() as i32).min(n as i32 - 1) {
                    // The sample's place along the gradient, in SVG units.
                    let (px, py) = ((x as f32 + 0.5 - ox) / s, (cy - oy) / s);
                    let t = (((px - fx) * gx + (py - fy) * gy) / g2).clamp(0.0, 1.0);
                    let mix = |k: usize| (f32::from(c0[k]) + (f32::from(c1[k]) - f32::from(c0[k])) * t).round() as u8;
                    buf[y as usize * n + x as usize] = [mix(0), mix(1), mix(2), 255];
                }
            }
        }
    }
    // Average each ss x ss block down to one pixel (straight alpha = covered share).
    let mut rgba = Vec::with_capacity(size * size * 4);
    for y in 0..size {
        for x in 0..size {
            let (mut sum, mut cnt) = ([0u32; 3], 0u32);
            for j in 0..ss {
                for i in 0..ss {
                    let p = buf[(y * ss + j) * n + x * ss + i];
                    if p[3] != 0 {
                        cnt += 1;
                        for k in 0..3 {
                            sum[k] += u32::from(p[k]);
                        }
                    }
                }
            }
            if cnt == 0 {
                rgba.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let a = 255 * cnt / (ss * ss) as u32;
                rgba.extend_from_slice(&[(sum[0] / cnt) as u8, (sum[1] / cnt) as u8, (sum[2] / cnt) as u8, a as u8]);
            }
        }
    }
    Ok(rgba)
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_mark_has_transparency_and_original_colors_at_window_and_package_sizes() {
        for size in [16, 24, 32, 48, 64, 128, 256] {
            let pixels = super::rgba(size).expect("valid size");
            assert_eq!(pixels.len(), size * size * 4);
            assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 0));
            assert!(pixels.as_chunks::<4>().0.iter().any(|p| p[3] == 255 && p[2] > p[0]));
        }
        assert!(super::rgba(0).is_err());
        assert!(super::rgba(2049).is_err());
    }

    /// Borderless: no white edge lines, and the gaps between faces are see-through. At 256 px
    /// the gap between the slab's left and right sides (SVG x 494..530) is transparent on a row
    /// through the slab, and no opaque pixel is white.
    #[test]
    fn shared_mark_is_borderless() {
        let size = 256;
        let pixels = super::rgba(size).expect("valid size");
        let px = |x: usize, y: usize| pixels.as_chunks::<4>().0[y * size + x];
        let s = size as f32 / 901.608;
        let gap_x = ((size as f32 - 789.816 * s) / 2.0 + (512.0 - 117.092) * s) as usize;
        let row = ((910.0 - 56.159) * s) as usize;
        assert_eq!(px(gap_x, row)[3], 0, "the seam between the slab's sides is a gap");
        assert!(px(gap_x - 8, row)[3] == 255 && px(gap_x + 8, row)[3] == 255, "faces on both sides of the gap");
        assert!(pixels.as_chunks::<4>().0.iter().all(|p| p[3] == 0 || p[0] < 200), "no white edges");
    }
}
