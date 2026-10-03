//! Opening an input file. Exports often arrive gzipped, so a file that starts
//! with the gzip magic bytes is decompressed on the fly, whatever its name.

use std::fs::File;
use std::io::{self, BufReader, Read, Seek};
use std::path::Path;

use flate2::read::MultiGzDecoder;

/// A file being read, plain or through a gzip decoder.
pub struct Input(Inner);

enum Inner {
    Plain(Counted),
    // MultiGz so concatenated gzip members (`cat a.gz b.gz`) read as one file.
    Gzip(Box<MultiGzDecoder<BufReader<Counted>>>),
}

/// Counts bytes taken from the file on disk, so progress for a gzipped input
/// can be measured against its compressed size.
struct Counted {
    file: File,
    read: u64,
}

impl Read for Counted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.file.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

impl Input {
    pub fn open(path: &Path) -> io::Result<Input> {
        let mut file = File::open(path)?;
        let mut magic = [0u8; 2];
        let n = file.read(&mut magic)?;
        file.rewind()?;
        let counted = Counted { file, read: 0 };
        Ok(Input(if n == 2 && magic == [0x1f, 0x8b] {
            Inner::Gzip(Box::new(MultiGzDecoder::new(BufReader::new(counted))))
        } else {
            Inner::Plain(counted)
        }))
    }

    /// Bytes read from disk so far, compressed bytes for a gzipped file.
    pub fn bytes_read(&self) -> u64 {
        match &self.0 {
            Inner::Plain(c) => c.read,
            Inner::Gzip(d) => d.get_ref().get_ref().read,
        }
    }
}

impl Read for Input {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match &mut self.0 {
            Inner::Plain(c) => c.read(buf),
            Inner::Gzip(d) => d.read(buf).map_err(|e| {
                io::Error::new(e.kind(), format!("gzip data is damaged or cut short ({e})"))
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn gzip_and_plain_read_the_same() {
        let dir = tempfile::tempdir().unwrap();
        let text = b"id,v\n1,a\n2,b\n";
        let plain = dir.path().join("x.csv");
        std::fs::write(&plain, text).unwrap();
        let gz = dir.path().join("x.csv.gz");
        let mut enc = flate2::write::GzEncoder::new(File::create(&gz).unwrap(), Default::default());
        enc.write_all(text).unwrap();
        enc.finish().unwrap();

        for p in [&plain, &gz] {
            let mut input = Input::open(p).unwrap();
            let mut out = Vec::new();
            input.read_to_end(&mut out).unwrap();
            assert_eq!(out, text);
            assert_eq!(input.bytes_read(), std::fs::metadata(p).unwrap().len());
        }
    }
}
