use std::sync::atomic::{AtomicU16, Ordering};

use rusttype::{Font as RTFont, GlyphId, Point, PositionedGlyph, Scale, ScaledGlyph};

use crate::layout::Frame;

const BUILT_IN_FONT: &[u8] = include_bytes!("../assets/fonts/monospace_typewriter.ttf");

static mut FONT_COUNT: AtomicU16 = AtomicU16::new(0);

pub(crate) type FontId = u16;
type LineHeight = f32;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Glyph {
    // pub(crate) glyph: PositionedGlyph<'static>,
    // pub(crate) ch: char,
    // pub(crate) font_id: FontId,
    pub glyph: PositionedGlyph<'static>,
    pub ch: char,
    pub font_id: FontId,
}

pub struct Font {
    pub default_size: f32,
    font_id: FontId,
    font: RTFont<'static>,
}

impl Font {
    pub fn new(bytes: Vec<u8>, default_size: f32) -> Self {
        let font_id = unsafe { FONT_COUNT.fetch_add(1, Ordering::Relaxed) };
        let font = RTFont::try_from_vec(bytes).unwrap();

        Font {
            font_id,
            font,
            default_size,
        }
    }

    pub fn load_default() -> Self {
        Font::new(BUILT_IN_FONT.to_owned(), 16.)
    }

    pub fn glyph(&self, ch: char, pos: [f32; 2], scale_factor: f64, size: Option<f32>) -> Glyph {
        let sf = scale_factor as f32;
        let size = size.unwrap_or(self.default_size);
        let scale = Scale {
            x: size * sf,
            y: size * sf,
        };
        let start = Point {
            x: pos[0] * sf,
            y: pos[1] * sf,
        };
        let g = self.font.glyph(ch); // TODO: bottleneck
        let g = g.scaled(scale);
        let g = g.positioned(start);

        Glyph {
            font_id: self.font_id,
            glyph: g,
            ch,
        }
    }

    pub fn layout_line<S: AsRef<str>>(
        &self,
        text: S,
        start: [f32; 2],
        scale_factor: f64,
        size: Option<f32>,
    ) -> Vec<Glyph> {
        let (_cur, glyphs) = self.layout_line_cur(text, start, scale_factor, size);
        glyphs
    }

    pub fn layout_line_cur<S: AsRef<str>, P: Into<Cursor>>(
        &self,
        text: S,
        start: P,
        scale_factor: f64,
        size: Option<f32>,
    ) -> (Cursor, Vec<Glyph>) {
        let sf = scale_factor as f32;
        let size = size.unwrap_or(self.default_size);
        let scale = Scale {
            x: size * sf,
            y: size * sf,
        };

        let mut cursor = start.into();

        let line_height = {
            let metrics = self.font.v_metrics(scale);
            ((metrics.ascent - metrics.descent) + metrics.line_gap) / sf
        };

        let glyphs = text
            .as_ref()
            .chars()
            .map(|ch| {
                let g = self.font.glyph(ch);
                let g = g.scaled(scale);
                cursor.kern(&self.font, scale, self.font_id, g.id());
                let p = Point {
                    x: cursor.pos[0] * sf,
                    y: cursor.pos[1] * sf,
                };
                cursor.advance(self.font_id, line_height, sf, &g);
                Glyph {
                    font_id: self.font_id,
                    glyph: g.positioned(p),
                    ch,
                }
            })
            .collect();

        (cursor, glyphs)
    }

    pub fn layout_wrapped<S: AsRef<str>, P: Into<Cursor>>(
        &self,
        text: S,
        start: P,
        scale_factor: f64,
        size: Option<f32>,
        max_x: f32,
        line_spacing: f32,
        align: Option<f32>,
    ) -> Vec<Glyph> {
        let (_cur, glyphs) =
            self.layout_wrapped_cur(text, start, scale_factor, size, max_x, line_spacing, align);
        glyphs
    }

    pub fn layout_wrapped_cur<S: AsRef<str>, P: Into<Cursor>>(
        &self,
        text: S,
        start: P,
        scale_factor: f64,
        size: Option<f32>,
        max_x: f32,
        line_spacing: f32,
        align: Option<f32>,
    ) -> (Cursor, Vec<Glyph>) {
        let align = align.unwrap_or(0.);

        let sf = scale_factor as f32;
        let size = size.unwrap_or(self.default_size);
        let scale = Scale {
            x: size * sf,
            y: size * sf,
        };

        let mut cursor = start.into();
        let min_x = cursor.original_start_pos[0];

        let line_height = {
            let metrics = self.font.v_metrics(scale);
            ((metrics.ascent - metrics.descent) + metrics.line_gap) / sf
        } + line_spacing;

        let mut glyphs = vec![];
        let mut words_in_line = vec![];
        let mut line_break_word = None;
        let mut line_end_cursor = cursor.clone();
        let mut visual_end_cursor = line_end_cursor.clone();

        let mut word_iter = WordIter::new(text.as_ref());

        loop {
            let word = word_iter.next();

            let mut end_line = word.is_none();

            if let Some(word) = word {
                if word == "\n" {
                    end_line = true;
                } else {
                    let mut word_cursor = line_end_cursor.clone();
                    for ch in word.chars() {
                        let g = self.font.glyph(ch);
                        let g = g.scaled(scale);
                        word_cursor.kern(&self.font, scale, self.font_id, g.id());
                        word_cursor.advance(self.font_id, line_height, sf, &g);
                    }

                    if word_cursor.pos[0] > max_x && line_end_cursor.pos[0] > min_x {
                        end_line = true;
                        if !word.trim_start().is_empty() {
                            line_break_word = Some(word);
                        }
                    } else {
                        words_in_line.push(word);
                        line_end_cursor = word_cursor.clone();
                        if !word.trim_start().is_empty() {
                            visual_end_cursor = line_end_cursor.clone();
                        }
                    }
                }
            }

            if end_line {
                let space_on_right = (max_x - visual_end_cursor.pos[0]).max(0.).round();
                let shift_to_align = space_on_right * align;
                cursor.pos[0] += shift_to_align;

                for word in words_in_line.drain(..) {
                    for ch in word.chars() {
                        let g = self.font.glyph(ch);
                        let id = g.id();
                        let g = g.scaled(scale);
                        cursor.kern(&self.font, scale, self.font_id, id);
                        let p = Point {
                            x: cursor.pos[0] * sf,
                            y: cursor.pos[1] * sf,
                        };
                        cursor.advance(self.font_id, line_height, sf, &g);
                        glyphs.push(Glyph {
                            font_id: self.font_id,
                            glyph: g.positioned(p),
                            ch,
                        });
                    }
                }

                if word.is_some() {
                    cursor.newline(line_height);
                }

                line_end_cursor = cursor.clone();
                visual_end_cursor = line_end_cursor.clone();

                if let Some(word) = line_break_word.take() {
                    words_in_line.push(word);

                    for ch in word.chars() {
                        let g = self.font.glyph(ch);
                        let g = g.scaled(scale);
                        line_end_cursor.kern(&self.font, scale, self.font_id, g.id());
                        line_end_cursor.advance(self.font_id, line_height, sf, &g);
                    }

                    if !word.trim_start().is_empty() {
                        visual_end_cursor = line_end_cursor.clone();
                    }
                }
            }

            if word.is_none() {
                break;
            }
        }

        (cursor, glyphs)
    }
}

#[derive(Debug, Clone)]
pub struct Cursor {
    pos: [f32; 2],
    original_start_pos: [f32; 2],
    max_x: f32,
    prev_glyph: Option<(FontId, GlyphId, LineHeight)>,
}

impl Cursor {
    pub const fn pos(&self) -> [f32; 2] {
        self.pos
    }

    pub const fn original_start_pos(&self) -> [f32; 2] {
        self.original_start_pos
    }

    pub fn end(&self) -> [f32; 2] {
        let line_height = self.prev_glyph.map(|x| x.2).unwrap_or(0.);
        [self.pos[0], self.pos[1] + line_height]
    }

    pub fn span_from<P: Into<[f32; 2]>>(&self, from: P) -> [f32; 2] {
        let [x0, y0] = from.into();
        let [x1, y1] = self.end();
        [x1 - x0, y1 - y0]
    }

    pub fn span(&self) -> [f32; 2] {
        self.span_from(self.original_start_pos())
    }

    pub fn frame(&self) -> Frame {
        let tl = self.original_start_pos();
        let [_, h] = self.span();
        let w = self.max_x - tl[0];
        Frame::new(tl, [w, h])
    }

    pub fn newline(&mut self, line_height: f32) {
        let ox = self.original_start_pos[0];
        let y = self.pos[1] + line_height;
        self.pos = [ox, y];
        self.prev_glyph = None;
    }

    pub(crate) fn kern(&mut self, font: &RTFont, scale: Scale, font_id: FontId, glyph_id: GlyphId) {
        if let Some((prev_font, prev_glyph, _)) = self.prev_glyph {
            if prev_font == font_id {
                self.pos[0] += font.pair_kerning(scale, prev_glyph, glyph_id); // TODO: bottleneck
                self.max_x = self.max_x.max(self.pos[0]);
            }
        }
    }

    pub(crate) fn advance(
        &mut self,
        font_id: FontId,
        line_height: f32,
        sf: f32,
        glyph: &ScaledGlyph,
    ) {
        let w = glyph.h_metrics().advance_width; // TODO: bottleneck
        self.prev_glyph = Some((font_id, glyph.id(), line_height));
        self.pos[0] += w / sf;
    }
}

impl Into<[f32; 2]> for Cursor {
    fn into(self) -> [f32; 2] {
        self.pos
    }
}

impl From<[f32; 2]> for Cursor {
    fn from(pos: [f32; 2]) -> Self {
        Cursor {
            pos,
            original_start_pos: pos,
            prev_glyph: None,
            max_x: pos[0],
        }
    }
}

struct WordIter<'a> {
    source: &'a str,
    iter: std::str::CharIndices<'a>,
    peek: Option<(usize, char)>,
}

impl<'a> WordIter<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut iter = source.char_indices();
        let peek = iter.next();
        WordIter { source, iter, peek }
    }
}

impl<'a> Iterator for WordIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((i, ch)) = self.peek {
            if ch == '\n' || ch == ' ' {
                self.peek = self.iter.next();
                Some(&self.source[i..(i + ch.len_utf8())])
            } else {
                loop {
                    self.peek = self.iter.next();
                    match self.peek {
                        None => return Some(&self.source[i..]),
                        Some((j, ' ' | '\n')) => return Some(&self.source[i..j]),
                        Some(_) => (),
                    }
                }
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_iter() {
        fn test(s: &str, expected: &[&str]) {
            let words = WordIter::new(s).collect::<Vec<_>>();
            assert_eq!(&words, expected);
        }

        test("", &[]);
        test("a", &["a"]);
        test("abc def", &["abc", " ", "def"]);
        test("  abc def", &[" ", " ", "abc", " ", "def"]);
        test("\nabc def", &["\n", "abc", " ", "def"]);
        test("abc \ndef\n", &["abc", " ", "\n", "def", "\n"]);
    }
}
