use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=assets/app.ico");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets_dir = manifest_dir.join("assets");
    let _ = fs::create_dir_all(&assets_dir);
    let ico_path = assets_dir.join("app.ico");

    let sizes = [16u32, 24, 32, 48, 64, 128];
    let images: Vec<(u32, Vec<u8>)> = sizes
        .iter()
        .map(|&size| (size, build_dib_icon_frame(size)))
        .collect();

    let ico_bytes = build_ico_file(&images);
    if fs::read(&ico_path).ok().as_deref() != Some(ico_bytes.as_slice()) {
        fs::write(&ico_path, &ico_bytes).expect("failed to write assets/app.ico");
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let res_path = out_dir.join("app_icon.res");
    let res_bytes = build_win32_res_file(&images);
    fs::write(&res_path, &res_bytes).expect("failed to write app_icon.res");

    println!(
        "cargo:rustc-link-arg-bin=edgelink-router={}",
        res_path.display()
    );
    println!(
        "cargo:rustc-link-arg-bin=edgelink-service={}",
        res_path.display()
    );
    println!(
        "cargo:rustc-link-arg-bin=edgelink-setup={}",
        res_path.display()
    );
}

fn build_dib_icon_frame(size: u32) -> Vec<u8> {
    let xor_bytes_len = (size * size * 4) as usize;
    let mask_row_bytes = ((size + 31) / 32 * 4) as usize;
    let and_bytes_len = mask_row_bytes * (size as usize);

    let mut buf = Vec::with_capacity(40 + xor_bytes_len + and_bytes_len);

    // BITMAPINFOHEADER (40 bytes)
    buf.extend_from_slice(&40u32.to_le_bytes()); // biSize
    buf.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    buf.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight (XOR + AND)
    buf.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    buf.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    buf.extend_from_slice(&0u32.to_le_bytes()); // biCompression (BI_RGB)
    buf.extend_from_slice(&(xor_bytes_len as u32).to_le_bytes()); // biSizeImage
    buf.extend_from_slice(&0i32.to_le_bytes()); // biXPelsPerMeter
    buf.extend_from_slice(&0i32.to_le_bytes()); // biYPelsPerMeter
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrUsed
    buf.extend_from_slice(&0u32.to_le_bytes()); // biClrImportant

    let mut and_mask = vec![0u8; and_bytes_len];
    let samples = if size <= 64 { 4 } else { 2 };

    // Bottom-up rows for DIB
    for y_inv in 0..size {
        let y = size - 1 - y_inv;
        for x in 0..size {
            let mut r_acc = 0.0f32;
            let mut g_acc = 0.0f32;
            let mut b_acc = 0.0f32;
            let mut a_acc = 0.0f32;

            for sy in 0..samples {
                for sx in 0..samples {
                    let u = (x as f32 + (sx as f32 + 0.5) / samples as f32) / size as f32;
                    let v = (y as f32 + (sy as f32 + 0.5) / samples as f32) / size as f32;
                    let (r, g, b, a) = sample_icon(u, v);
                    r_acc += r * a;
                    g_acc += g * a;
                    b_acc += b * a;
                    a_acc += a;
                }
            }

            let inv = 1.0 / (samples * samples) as f32;
            let alpha = (a_acc * inv).clamp(0.0, 1.0);
            let (r, g, b) = if alpha > 1e-4 {
                (
                    (r_acc * inv / alpha).clamp(0.0, 1.0),
                    (g_acc * inv / alpha).clamp(0.0, 1.0),
                    (b_acc * inv / alpha).clamp(0.0, 1.0),
                )
            } else {
                (0.0, 0.0, 0.0)
            };

            let b_u8 = (b * 255.0 + 0.5) as u8;
            let g_u8 = (g * 255.0 + 0.5) as u8;
            let r_u8 = (r * 255.0 + 0.5) as u8;
            let a_u8 = (alpha * 255.0 + 0.5) as u8;

            buf.push(b_u8);
            buf.push(g_u8);
            buf.push(r_u8);
            buf.push(a_u8);

            if a_u8 < 16 {
                let byte_idx = (y_inv as usize) * mask_row_bytes + (x as usize / 8);
                let bit_idx = 7 - (x as usize % 8);
                and_mask[byte_idx] |= 1 << bit_idx;
            }
        }
    }

    buf.extend_from_slice(&and_mask);
    buf
}

fn sample_icon(u: f32, v: f32) -> (f32, f32, f32, f32) {
    // 1. Rounded-rect squircle SDF
    let cx = u - 0.5;
    let cy = v - 0.5;
    let half = 0.44;
    let corner_r = 0.19;
    let dx = cx.abs() - (half - corner_r);
    let dy = cy.abs() - (half - corner_r);
    let outside = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt();
    let inside = dx.max(dy).min(0.0);
    let bg_dist = outside + inside - corner_r;

    let aa = 0.012;
    let bg_alpha = ((0.0 - bg_dist) / aa + 0.5).clamp(0.0, 1.0);
    if bg_alpha <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }

    // Dark Zinc/Obsidian background with subtle top-left cyan glow
    let t = (u * 0.4 + v * 0.6).clamp(0.0, 1.0);
    let mut r = lerp(0.06, 0.035, t);
    let mut g = lerp(0.09, 0.04, t);
    let mut b = lerp(0.14, 0.06, t);

    // Subtle radial glow around center
    let glow_d = ((u - 0.45).powi(2) + (v - 0.38).powi(2)).sqrt();
    let glow = (1.0 - glow_d / 0.48).clamp(0.0, 1.0).powi(2) * 0.22;
    r += 0.05 * glow;
    g += 0.55 * glow;
    b += 0.85 * glow;

    // Border highlight near edge of squircle
    let border_band = (1.0 - (bg_dist + 0.024).abs() / 0.024).clamp(0.0, 1.0);
    let border_str = border_band * lerp(0.55, 0.18, v);
    r = lerp(r, 0.22, border_str);
    g = lerp(g, 0.74, border_str);
    b = lerp(b, 0.97, border_str);

    // 2. Route traces (left hub -> center split -> upper-right & lower-right nodes)
    let p_left = (0.26f32, 0.50f32);
    let p_mid = (0.47f32, 0.50f32);
    let p_top = (0.71f32, 0.29f32);
    let p_bot = (0.71f32, 0.71f32);

    let d_trunk = dist_to_segment((u, v), p_left, p_mid);
    let d_upper = dist_to_segment((u, v), p_mid, p_top);
    let d_lower = dist_to_segment((u, v), p_mid, p_bot);

    // Lower branch (electric blue #3b82f6)
    let lower_alpha = ((0.036 - d_lower) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.23, lower_alpha);
    g = lerp(g, 0.51, lower_alpha);
    b = lerp(b, 0.96, lower_alpha);

    // Main trunk & upper branch (cyan #38bdf8 -> emerald #10b981)
    let main_route_d = d_trunk.min(d_upper);
    let route_alpha = ((0.042 - main_route_d) / aa + 0.5).clamp(0.0, 1.0);
    let route_t = ((u - 0.26) / 0.45).clamp(0.0, 1.0);
    let rr = lerp(0.22, 0.06, route_t);
    let rg = lerp(0.74, 0.73, route_t);
    let rb = lerp(0.97, 0.51, route_t);
    r = lerp(r, rr, route_alpha);
    g = lerp(g, rg, route_alpha);
    b = lerp(b, rb, route_alpha);

    // 3. Nodes
    // Left Host Node (Cyan #38bdf8 with white center)
    let d_n1 = ((u - p_left.0).powi(2) + (v - p_left.1).powi(2)).sqrt();
    let n1_outer = ((0.095 - d_n1) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.22, n1_outer);
    g = lerp(g, 0.74, n1_outer);
    b = lerp(b, 0.97, n1_outer);
    let n1_inner = ((0.042 - d_n1) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.95, n1_inner);
    g = lerp(g, 0.99, n1_inner);
    b = lerp(b, 1.00, n1_inner);

    // Top-Right Active Board Node (Emerald #10b981 with bright core)
    let d_n2 = ((u - p_top.0).powi(2) + (v - p_top.1).powi(2)).sqrt();
    let n2_outer = ((0.095 - d_n2) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.06, n2_outer);
    g = lerp(g, 0.78, n2_outer);
    b = lerp(b, 0.52, n2_outer);
    let n2_inner = ((0.042 - d_n2) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.85, n2_inner);
    g = lerp(g, 1.00, n2_inner);
    b = lerp(b, 0.93, n2_inner);

    // Bottom-Right Secondary Node (Blue #3b82f6 with light core)
    let d_n3 = ((u - p_bot.0).powi(2) + (v - p_bot.1).powi(2)).sqrt();
    let n3_outer = ((0.082 - d_n3) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.23, n3_outer);
    g = lerp(g, 0.51, n3_outer);
    b = lerp(b, 0.96, n3_outer);
    let n3_inner = ((0.035 - d_n3) / aa + 0.5).clamp(0.0, 1.0);
    r = lerp(r, 0.88, n3_inner);
    g = lerp(g, 0.94, n3_inner);
    b = lerp(b, 1.00, n3_inner);

    (r, g, b, bg_alpha)
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let vx = b.0 - a.0;
    let vy = b.1 - a.1;
    let wx = p.0 - a.0;
    let wy = p.1 - a.1;
    let c1 = vx * wx + vy * wy;
    if c1 <= 0.0 {
        return (wx * wx + wy * wy).sqrt();
    }
    let c2 = vx * vx + vy * vy;
    if c2 <= c1 {
        return ((p.0 - b.0).powi(2) + (p.1 - b.1).powi(2)).sqrt();
    }
    let t = c1 / c2;
    let px = a.0 + t * vx;
    let py = a.1 + t * vy;
    ((p.0 - px).powi(2) + (p.1 - py).powi(2)).sqrt()
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn build_ico_file(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let count = images.len() as u16;
    let mut out = Vec::new();
    // ICONDIR header (6 bytes)
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // Type = 1 (ICON)
    out.extend_from_slice(&count.to_le_bytes());

    let mut offset = 6u32 + (images.len() as u32) * 16;
    for (size, data) in images {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        out.push(dim); // bWidth
        out.push(dim); // bHeight
        out.push(0); // bColorCount
        out.push(0); // bReserved
        out.extend_from_slice(&1u16.to_le_bytes()); // wPlanes
        out.extend_from_slice(&32u16.to_le_bytes()); // wBitCount
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()); // dwBytesInRes
        out.extend_from_slice(&offset.to_le_bytes()); // dwImageOffset
        offset += data.len() as u32;
    }

    for (_, data) in images {
        out.extend_from_slice(data);
    }
    out
}

fn build_win32_res_file(images: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut res = Vec::new();

    // 1. Standard 32-byte empty resource header (Win32 .res magic)
    write_res_header(&mut res, 0, 0, 0, 0, 0);

    // 2. Individual RT_ICON (type = 3) resources with IDs 1..=N
    for (idx, (_, data)) in images.iter().enumerate() {
        let icon_id = (idx + 1) as u16;
        write_res_record(&mut res, 3, icon_id, data);
    }

    // 3. RT_GROUP_ICON (type = 14) resource with ID = 1
    let mut grp = Vec::with_capacity(6 + images.len() * 14);
    grp.extend_from_slice(&0u16.to_le_bytes()); // idReserved
    grp.extend_from_slice(&1u16.to_le_bytes()); // idType = 1
    grp.extend_from_slice(&(images.len() as u16).to_le_bytes()); // idCount
    for (idx, (size, data)) in images.iter().enumerate() {
        let dim = if *size >= 256 { 0u8 } else { *size as u8 };
        let icon_id = (idx + 1) as u16;
        grp.push(dim); // bWidth
        grp.push(dim); // bHeight
        grp.push(0); // bColorCount
        grp.push(0); // bReserved
        grp.extend_from_slice(&1u16.to_le_bytes()); // wPlanes
        grp.extend_from_slice(&32u16.to_le_bytes()); // wBitCount
        grp.extend_from_slice(&(data.len() as u32).to_le_bytes()); // dwBytesInRes
        grp.extend_from_slice(&icon_id.to_le_bytes()); // nID
    }
    write_res_record(&mut res, 14, 1, &grp);

    res
}

fn write_res_header(
    out: &mut Vec<u8>,
    data_size: u32,
    type_id: u16,
    name_id: u16,
    memory_flags: u16,
    language_id: u16,
) {
    out.extend_from_slice(&data_size.to_le_bytes()); // DataSize
    out.extend_from_slice(&32u32.to_le_bytes()); // HeaderSize
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Ordinal Type marker
    out.extend_from_slice(&type_id.to_le_bytes()); // Type ID
    out.extend_from_slice(&0xFFFFu16.to_le_bytes()); // Ordinal Name marker
    out.extend_from_slice(&name_id.to_le_bytes()); // Name ID
    out.extend_from_slice(&0u32.to_le_bytes()); // DataVersion
    out.extend_from_slice(&memory_flags.to_le_bytes()); // MemoryFlags
    out.extend_from_slice(&language_id.to_le_bytes()); // LanguageId
    out.extend_from_slice(&0u32.to_le_bytes()); // Version
    out.extend_from_slice(&0u32.to_le_bytes()); // Characteristics
}

fn write_res_record(out: &mut Vec<u8>, type_id: u16, name_id: u16, data: &[u8]) {
    write_res_header(out, data.len() as u32, type_id, name_id, 0x1010, 0x0409);
    out.extend_from_slice(data);
    while out.len() % 4 != 0 {
        out.push(0);
    }
}
