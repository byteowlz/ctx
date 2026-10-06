use std::io::{BufWriter, Write};

use image::{ColorType, ImageResult, RgbImage, codecs::jpeg::JpegEncoder};

/// JPEG emits tiny writes; buffer them and explicitly report final flush failures.
pub(crate) fn write_jpeg(image: &RgbImage, output: impl Write, quality: u8) -> ImageResult<()> {
    let mut output = BufWriter::new(output);
    JpegEncoder::new_with_quality(&mut output, quality.clamp(1, 100)).encode(
        image.as_raw(),
        image.width(),
        image.height(),
        ColorType::Rgb8.into(),
    )?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{self, Write};

    use image::{ImageBuffer, Rgb};

    use super::*;

    #[derive(Default)]
    struct RecordingWriter {
        bytes: Vec<u8>,
        writes: usize,
        fail_write: bool,
        fail_flush: bool,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            if self.fail_write {
                return Err(io::Error::other("injected write failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.fail_flush {
                return Err(io::Error::other("injected flush failure"));
            }
            Ok(())
        }
    }

    fn fixture() -> RgbImage {
        ImageBuffer::from_fn(128, 128, |x, y| {
            Rgb([
                (x * 37 + y * 17) as u8,
                (x * 53 + y * 29) as u8,
                (x * 97 + y * 71) as u8,
            ])
        })
    }

    #[test]
    fn jpeg_output_batches_writes_and_decodes() {
        let mut output = RecordingWriter::default();
        write_jpeg(&fixture(), &mut output, 80).unwrap();
        assert!(
            output.bytes.len() > 4096,
            "fixture must exercise more than a JPEG header"
        );
        assert!(
            output.writes <= 4,
            "JPEG byte-at-a-time IO returned: {} writes for {} bytes",
            output.writes,
            output.bytes.len()
        );
        let decoded = image::load_from_memory(&output.bytes).unwrap();
        assert_eq!(decoded.width(), 128);
        assert_eq!(decoded.height(), 128);
    }

    #[test]
    fn jpeg_output_reports_buffered_write_and_flush_errors() {
        for (fail_write, fail_flush) in [(true, false), (false, true)] {
            let mut output = RecordingWriter {
                fail_write,
                fail_flush,
                ..Default::default()
            };
            assert!(write_jpeg(&fixture(), &mut output, 80).is_err());
        }
    }

    #[test]
    fn jpeg_quality_clamping_is_unchanged() {
        for (input, clamped) in [(0, 1), (255, 100)] {
            let mut actual = Vec::new();
            let mut expected = Vec::new();
            write_jpeg(&fixture(), &mut actual, input).unwrap();
            write_jpeg(&fixture(), &mut expected, clamped).unwrap();
            assert_eq!(actual, expected);
        }
    }
}
