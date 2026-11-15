#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyGeom {
    pub x: f32,
    pub y: f32,
    pub width: f32,
}

const ROWS: [&[(u8, f32)]; 6] = [
    &[
        (53, 1.0), (122, 1.0), (120, 1.0), (99, 1.0), (118, 1.0), (96, 1.0), (97, 1.0),
        (98, 1.0), (100, 1.0), (101, 1.0), (109, 1.0), (103, 1.0), (111, 1.0),
    ],
    &[
        (50, 1.0), (18, 1.0), (19, 1.0), (20, 1.0), (21, 1.0), (23, 1.0), (22, 1.0),
        (26, 1.0), (28, 1.0), (25, 1.0), (29, 1.0), (27, 1.0), (24, 1.0), (51, 2.0),
    ],
    &[
        (48, 1.5), (12, 1.0), (13, 1.0), (14, 1.0), (15, 1.0), (17, 1.0), (16, 1.0),
        (32, 1.0), (34, 1.0), (31, 1.0), (35, 1.0), (33, 1.0), (30, 1.0), (42, 1.5),
    ],
    &[
        (57, 1.75), (0, 1.0), (1, 1.0), (2, 1.0), (3, 1.0), (5, 1.0), (4, 1.0),
        (38, 1.0), (40, 1.0), (37, 1.0), (41, 1.0), (39, 1.0), (36, 2.25),
    ],
    &[
        (56, 2.25), (6, 1.0), (7, 1.0), (8, 1.0), (9, 1.0), (11, 1.0), (45, 1.0),
        (46, 1.0), (43, 1.0), (47, 1.0), (44, 1.0), (60, 1.75), (126, 1.0),
    ],
    &[
        (63, 1.0), (59, 1.0), (58, 1.0), (55, 1.25), (49, 6.25), (54, 1.25), (61, 1.0),
        (123, 1.0), (125, 1.0), (124, 1.0),
    ],
];

const ROW_UNITS: f32 = 15.0;

pub fn geometry(code: u8) -> Option<KeyGeom> {
    for (row, keys) in ROWS.iter().enumerate() {
        let mut x = 0.0;
        for &(c, w) in keys.iter() {
            if c == code {
                return Some(KeyGeom {
                    x: ((x + 0.5 * w) / ROW_UNITS).min(1.0),
                    y: (row as f32 + 0.5) / ROWS.len() as f32,
                    width: w,
                });
            }
            x += w;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys() {
        let space = geometry(49).unwrap();
        assert_eq!(space.width, 6.25);
        assert!((space.x - 0.5).abs() < 0.1);
        assert_eq!(geometry(36).unwrap().width, 2.25);
        let a = geometry(0).unwrap();
        let l = geometry(37).unwrap();
        assert_eq!(a.y, l.y);
        assert!(a.x < l.x);
        assert!(geometry(200).is_none());
        assert!(geometry(65).is_none());
    }

    #[test]
    fn rows_have_unique_codes() {
        let mut seen = [false; 256];
        for keys in ROWS {
            for &(c, _) in keys {
                assert!(!seen[c as usize], "duplicate {c}");
                seen[c as usize] = true;
            }
        }
    }
}
