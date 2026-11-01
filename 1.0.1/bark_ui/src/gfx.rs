// Anti-aliased shapes drawn straight into a 32-bit DIB (0x00RRGGBB).
pub struct Canvas {
    pub p: *mut u32,
    pub w: i32,
    pub h: i32,
}

pub fn rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | b as u32
}

pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |sh: u32| {
        let (x, y) = (((a >> sh) & 255) as f32, ((b >> sh) & 255) as f32);
        (x + (y - x) * t).round() as u32
    };
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

impl Canvas {
    unsafe fn blend(&self, x: i32, y: i32, c: u32, a: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return;
        }
        let px = self.p.add((y * self.w + x) as usize);
        *px = mix(*px, c, a);
    }

    pub unsafe fn fill(&self, c: u32) {
        std::slice::from_raw_parts_mut(self.p, (self.w * self.h) as usize).fill(c);
    }

    pub unsafe fn rect(&self, l: i32, t: i32, r: i32, b: i32, c: u32) {
        for y in t.max(0)..b.min(self.h) {
            let row = self.p.add((y * self.w) as usize);
            for x in l.max(0)..r.min(self.w) {
                *row.add(x as usize) = c;
            }
        }
    }

    pub unsafe fn border(&self, c: u32) {
        for x in 0..self.w {
            *self.p.add(x as usize) = c;
            *self.p.add(((self.h - 1) * self.w + x) as usize) = c;
        }
        for y in 0..self.h {
            *self.p.add((y * self.w) as usize) = c;
            *self.p.add((y * self.w + self.w - 1) as usize) = c;
        }
    }

    pub unsafe fn rrect(&self, l: f32, t: f32, r: f32, b: f32, rad: f32, c: u32) {
        let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
        let (hx, hy) = ((r - l) / 2.0, (b - t) / 2.0);
        if hx <= 0.0 || hy <= 0.0 {
            return;
        }
        let rad = rad.min(hx).min(hy);
        for y in (t.floor() as i32 - 1)..=(b.ceil() as i32 + 1) {
            for x in (l.floor() as i32 - 1)..=(r.ceil() as i32 + 1) {
                let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let (qx, qy) = (px.abs() - (hx - rad), py.abs() - (hy - rad));
                let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rad;
                let a = (0.5 - d).clamp(0.0, 1.0);
                if a > 0.0 {
                    self.blend(x, y, c, a);
                }
            }
        }
    }

    /// Rounded rectangle with a separate radius per corner: [top-left, top-right, bottom-right, bottom-left].
    pub unsafe fn rrect4(&self, l: f32, t: f32, r: f32, b: f32, rad: [f32; 4], c: u32) {
        let (cx, cy) = ((l + r) / 2.0, (t + b) / 2.0);
        let (hx, hy) = ((r - l) / 2.0, (b - t) / 2.0);
        if hx <= 0.0 || hy <= 0.0 {
            return;
        }
        for y in (t.floor() as i32 - 1)..=(b.ceil() as i32 + 1) {
            for x in (l.floor() as i32 - 1)..=(r.ceil() as i32 + 1) {
                let (px, py) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
                let rr = match (px >= 0.0, py >= 0.0) {
                    (false, false) => rad[0],
                    (true, false) => rad[1],
                    (true, true) => rad[2],
                    (false, true) => rad[3],
                }
                .max(0.0)
                .min(hx)
                .min(hy);
                let (qx, qy) = (px.abs() - (hx - rr), py.abs() - (hy - rr));
                let d = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() + qx.max(qy).min(0.0) - rr;
                let a = (0.5 - d).clamp(0.0, 1.0);
                if a > 0.0 {
                    self.blend(x, y, c, a);
                }
            }
        }
    }

    /// Paints colour `c` through a coverage mask. `src` is a w*h 32-bit buffer whose blue channel is the coverage
    /// (white text on black). Used so a glyph has the same weight whatever colour it is drawn in.
    pub unsafe fn mask(&self, x0: i32, y0: i32, w: i32, h: i32, src: *const u32, c: u32) {
        for y in 0..h {
            for x in 0..w {
                let cov = (*src.add((y * w + x) as usize) & 0xFF) as f32 / 255.0;
                if cov > 0.0 {
                    self.blend(x0 + x, y0 + y, c, cov);
                }
            }
        }
    }

    pub unsafe fn circle(&self, cx: f32, cy: f32, rad: f32, c: u32) {
        self.rrect(cx - rad, cy - rad, cx + rad, cy + rad, rad, c);
    }
}
