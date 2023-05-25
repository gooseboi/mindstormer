use std::io::{self, BufRead, Read};

pub struct VecReadWrapper {
    buf: Vec<u8>,
    start: usize,
}

impl VecReadWrapper {
    pub fn new(buf: Vec<u8>) -> Self {
        Self { buf, start: 0 }
    }
}

impl Read for VecReadWrapper {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut b = &self.buf.as_slice()[self.start..];
        match b.read(buf) {
            Ok(n) => {
                self.start += n;
                Ok(n)
            }
            e @ Err(_) => e,
        }
    }
}

impl BufRead for VecReadWrapper {
    #[inline]
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        Ok(&self.buf.as_slice()[self.start..])
    }

    #[inline]
    fn consume(&mut self, amt: usize) {
        self.start += amt;
    }
}
