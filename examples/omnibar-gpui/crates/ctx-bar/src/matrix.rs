//! Departure Mono's real CFF outlines, sampled at 11px and displayed at 3x.
//! No generated/alphabet-substitute glyphs and no host capture or font install.
use std::sync::OnceLock;

use fontdue::{Font, FontSettings, Metrics};

pub(crate) const FONT_BYTES: &[u8] =
    include_bytes!("../../../assets/fonts/DepartureMono-Regular.otf");
// SFNT name IDs 1/16 and CoreText's CTFontCopyFamilyName both confirm this
// native family; it is not inferred from the browser's @font-face alias.
pub(crate) const FONT_FAMILY: &str = "Departure Mono";
pub(crate) const PITCH: f32 = 3.0;
/// Fraction of pitch reserved between cells, not a logical-pixel distance.
pub(crate) const GAP: f32 = 0.25;
pub(crate) const CORNER: f32 = 0.34;
pub(crate) const FONT_SIZE: f32 = 33.0;
const RASTER_SIZE: f32 = 11.0;
const MAX_CELLS: usize = 262_144;

pub(crate) struct Glyph {
    pub metrics: Metrics,
    pub bitmap: Vec<u8>,
    pub index: u16,
}

pub(crate) struct Atlas {
    glyphs: Vec<Glyph>,
}

impl Atlas {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let font = Font::from_bytes(bytes, FontSettings::default()).map_err(str::to_owned)?;
        let mut glyphs = Vec::with_capacity(96);
        // Ellipsis is an actual font glyph, eligible only in the empty placeholder.
        for ch in (b' '..=b'~').map(char::from).chain(std::iter::once('…')) {
            if !font.has_glyph(ch) {
                return Err(format!("Departure Mono has no glyph {ch:?}"));
            }
            let (metrics, bitmap) = font.rasterize(ch, RASTER_SIZE);
            if ch != ' ' && !bitmap.iter().any(|coverage| *coverage >= 128) {
                return Err(format!("Departure Mono raster is empty for {ch:?}"));
            }
            glyphs.push(Glyph {
                metrics,
                bitmap,
                index: font.lookup_glyph_index(ch),
            });
        }
        Ok(Self { glyphs })
    }

    pub fn glyph(&self, ch: char) -> Option<&Glyph> {
        let index = match ch {
            ' '..='~' => ch as usize - ' ' as usize,
            '…' => 95,
            _ => return None,
        };
        self.glyphs.get(index)
    }

    /// Entered values remain printable ASCII/LTR; an empty placeholder may also
    /// use the real ellipsis glyph. This does not expand IME/value support.
    pub fn supports(&self, text: &str, placeholder: bool) -> bool {
        (placeholder || text.is_ascii()) && text.chars().all(|ch| self.glyph(ch).is_some())
    }
}

pub(crate) fn atlas() -> Option<&'static Atlas> {
    static ATLAS: OnceLock<Result<Atlas, String>> = OnceLock::new();
    ATLAS
        .get_or_init(|| Atlas::decode(FONT_BYTES))
        .as_ref()
        .ok()
}

/// All coordinates are local to the canvas in logical pixels, not device pixels.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    fn contains_cell(self, cell: Self) -> bool {
        cell.x >= self.x
            && cell.y >= self.y
            && cell.x + cell.width <= self.x + self.width
            && cell.y + cell.height <= self.y + self.height
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Ink {
    #[default]
    Unlit,
    Selection,
    Letter,
    SelectedLetter,
    Caret,
}

pub(crate) struct Mask {
    columns: usize,
    rows: usize,
    pub cells: Vec<Ink>,
}

impl Mask {
    /// Partial edge cells are omitted, never stretched or clipped into rectangles.
    pub fn new(width: f32, height: f32) -> Option<Self> {
        if !width.is_finite() || !height.is_finite() || width < PITCH || height < PITCH {
            return None;
        }
        let columns = (width / PITCH).floor() as usize;
        let rows = (height / PITCH).floor() as usize;
        let count = columns.checked_mul(rows)?;
        if count > MAX_CELLS {
            return None;
        }
        Some(Self {
            columns,
            rows,
            cells: vec![Ink::Unlit; count],
        })
    }

    pub fn cell(&self, index: usize) -> Rect {
        Rect {
            x: (index % self.columns) as f32 * PITCH + PITCH * GAP / 2.0,
            y: (index / self.columns) as f32 * PITCH + PITCH * GAP / 2.0,
            width: PITCH * (1.0 - GAP),
            height: PITCH * (1.0 - GAP),
        }
    }

    pub fn rectangle(&mut self, rect: Rect, clip: Rect, ink: Ink) {
        for index in 0..self.cells.len() {
            let cell = self.cell(index);
            // Native caret is narrower than a cell: test centers against a
            // one-pitch caret supplied by the caller. Selection stays grid-only.
            let center_x = cell.x + cell.width / 2.0;
            let center_y = cell.y + cell.height / 2.0;
            if center_x >= rect.x
                && center_x < rect.x + rect.width
                && center_y >= rect.y
                && center_y < rect.y + rect.height
                && clip.contains_cell(cell)
            {
                self.cells[index] = ink;
            }
        }
    }

    /// x and baseline come from each native glyph, including native scroll.
    pub fn glyph(&mut self, glyph: &Glyph, x: f32, baseline: f32, clip: Rect) {
        let metrics = glyph.metrics;
        let column = ((x + metrics.xmin as f32 * PITCH) / PITCH).round() as isize;
        let row = ((baseline - (metrics.ymin as f32 + metrics.height as f32) * PITCH) / PITCH)
            .round() as isize;
        for (index, coverage) in glyph.bitmap.iter().enumerate() {
            if *coverage < 128 {
                continue;
            }
            let col = column + (index % metrics.width) as isize;
            let row = row + (index / metrics.width) as isize;
            if col < 0 || row < 0 || col >= self.columns as isize || row >= self.rows as isize {
                continue;
            }
            let target = row as usize * self.columns + col as usize;
            if clip.contains_cell(self.cell(target)) {
                self.cells[target] = if self.cells[target] == Ink::Selection {
                    Ink::SelectedLetter
                } else {
                    Ink::Letter
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 90.0,
            height: 63.0,
        }
    }

    #[test]
    fn official_cff_font_decodes_and_covers_printable_ascii() {
        assert_eq!(&FONT_BYTES[..4], b"OTTO");
        let atlas = Atlas::decode(FONT_BYTES).unwrap();
        for ch in ' '..='~' {
            let glyph = atlas.glyph(ch).unwrap();
            assert_ne!(glyph.index, 0);
            assert_eq!(
                glyph.bitmap.len(),
                glyph.metrics.width * glyph.metrics.height
            );
            assert!(glyph.metrics.advance_width > 0.0);
            if ch != ' ' {
                assert!(glyph.bitmap.iter().any(|coverage| *coverage >= 128));
            }
        }
        assert!(atlas.glyph('é').is_none());
        assert!(atlas.glyph('\n').is_none());
        assert!(Atlas::decode(b"not a font").is_err());
    }

    #[test]
    fn ellipsis_placeholder_uses_real_font_but_non_ascii_values_fall_back() {
        let atlas = atlas().unwrap();
        assert!(atlas.supports("Type…", true));
        assert!(atlas.supports("Type...", true));
        assert!(atlas.supports("ctx theme", false));
        assert!(!atlas.supports("Type…", false));
        assert!(!atlas.supports("é", true));
        assert!(!atlas.supports("hello\n", false));
        let glyph = atlas.glyph('…').unwrap();
        assert_ne!(glyph.index, 0);
        assert!(glyph.bitmap.iter().any(|coverage| *coverage >= 128));
        assert_ne!(glyph.index, atlas.glyph('.').unwrap().index);
        let mut mask = Mask::new(90.0, 63.0).unwrap();
        mask.glyph(glyph, 18.0, 36.0, clip());
        assert!(mask.cells.contains(&Ink::Letter));
    }

    #[test]
    fn registration_family_matches_embedded_sfnt_names() {
        // Verify name IDs without adding a second parser dependency. This pinned
        // SFNT contains Unicode Windows records (fonttools/CoreText also checked).
        let u16be = |bytes: &[u8]| u16::from_be_bytes(bytes[..2].try_into().unwrap()) as usize;
        let table_count = u16be(&FONT_BYTES[4..]);
        let record = FONT_BYTES[12..]
            .chunks_exact(16)
            .take(table_count)
            .find(|record| &record[..4] == b"name")
            .unwrap();
        let offset = u32::from_be_bytes(record[8..12].try_into().unwrap()) as usize;
        let table = &FONT_BYTES[offset..];
        let string_offset = u16be(&table[4..]);
        for (name_id, expected) in [
            (1, FONT_FAMILY),
            (16, FONT_FAMILY),
            (6, "DepartureMono-Regular"),
        ] {
            let name = table[6..]
                .chunks_exact(12)
                .take(u16be(&table[2..]))
                .find(|record| u16be(record) == 3 && u16be(&record[6..]) == name_id)
                .unwrap();
            let start = string_offset + u16be(&name[10..]);
            let bytes = &table[start..start + u16be(&name[8..])];
            let utf16: Vec<_> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_be_bytes(pair.try_into().unwrap()))
                .collect();
            assert_eq!(String::from_utf16(&utf16).unwrap(), expected);
        }
        let font = Font::from_bytes(FONT_BYTES, FontSettings::default()).unwrap();
        assert_eq!(font.name(), Some("Departure Mono Regular"));
    }

    #[test]
    fn real_letters_have_distinct_nonempty_bitmaps() {
        let atlas = atlas().unwrap();
        assert_ne!(
            atlas.glyph('a').unwrap().bitmap,
            atlas.glyph('b').unwrap().bitmap
        );
        assert_ne!(
            atlas.glyph('a').unwrap().bitmap,
            atlas.glyph('A').unwrap().bitmap
        );
        let mut mask = Mask::new(90.0, 63.0).unwrap();
        mask.glyph(atlas.glyph('g').unwrap(), 18.0, 36.0, clip());
        assert!(mask.cells.contains(&Ink::Letter));
    }

    #[test]
    fn all_layers_use_identical_separated_bounded_cells() {
        let mut mask = Mask::new(91.2, 64.0).unwrap();
        mask.rectangle(
            Rect {
                x: 18.0,
                y: 15.0,
                width: 36.0,
                height: 33.0,
            },
            clip(),
            Ink::Selection,
        );
        mask.glyph(atlas().unwrap().glyph('M').unwrap(), 18.0, 36.0, clip());
        mask.rectangle(
            Rect {
                x: 54.0,
                y: 15.0,
                width: PITCH,
                height: 33.0,
            },
            clip(),
            Ink::Caret,
        );
        for ink in [Ink::Unlit, Ink::Selection, Ink::SelectedLetter, Ink::Caret] {
            assert!(mask.cells.contains(&ink), "missing {ink:?}");
        }
        for index in 0..mask.cells.len() {
            let cell = mask.cell(index);
            assert_eq!(cell.width, 2.25);
            assert_eq!(cell.width, cell.height);
            assert!(clip().contains_cell(cell));
            if index % mask.columns != mask.columns - 1 {
                assert_eq!(mask.cell(index + 1).x - cell.x - cell.width, PITCH * GAP);
            }
        }
        const { assert!(CORNER < PITCH * (1.0 - GAP) / 2.0) };
    }

    #[test]
    fn native_positions_are_independent_and_clip_negative_scroll() {
        let atlas = atlas().unwrap();
        let glyph = atlas.glyph('X').unwrap();
        let mut first = Mask::new(90.0, 63.0).unwrap();
        let mut shifted = Mask::new(90.0, 63.0).unwrap();
        first.glyph(glyph, 6.0, 36.0, clip());
        shifted.glyph(glyph, 21.0, 36.0, clip());
        let first_col = first
            .cells
            .iter()
            .position(|ink| *ink == Ink::Letter)
            .unwrap()
            % first.columns;
        let shifted_col = shifted
            .cells
            .iter()
            .position(|ink| *ink == Ink::Letter)
            .unwrap()
            % shifted.columns;
        assert_eq!(shifted_col - first_col, 5);
        let small_clip = Rect {
            x: 9.0,
            y: 9.0,
            width: 27.0,
            height: 39.0,
        };
        shifted.glyph(glyph, -6.0, 36.0, small_clip);
        let mut clipped = Mask::new(90.0, 63.0).unwrap();
        clipped.glyph(glyph, -6.0, 36.0, small_clip);
        for (index, ink) in clipped.cells.iter().enumerate() {
            if *ink != Ink::Unlit {
                assert!(small_clip.contains_cell(clipped.cell(index)));
            }
        }
        clipped.glyph(glyph, 1000.0, -1000.0, small_clip);
        assert!(Mask::new(f32::NAN, 64.0).is_none());
        assert!(Mask::new(1e9, 1e9).is_none());
        assert!(Mask::new(0.0, 64.0).is_none());
    }
}
