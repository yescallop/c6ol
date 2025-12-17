pub struct NibbleWriter<'a> {
    buf: &'a mut Vec<u8>,
    pending: Option<u8>,
}

impl<'a> NibbleWriter<'a> {
    pub fn new(buf: &'a mut Vec<u8>) -> Self {
        Self { buf, pending: None }
    }

    fn write_u4(&mut self, n: u8) {
        if let Some(p) = self.pending.take() {
            self.buf.push(p | (n << 4));
        } else {
            self.pending = Some(n);
        }
    }

    pub fn write_u3(&mut self, n: u8) {
        assert!(n < 8);
        self.write_u4(n);
    }

    pub fn write_u32_varint(&mut self, mut n: u32) {
        while n >= 8 {
            self.write_u4((n as u8 & 7) | 8);
            n >>= 3;
        }
        self.write_u4(n as u8);
    }
}

impl Drop for NibbleWriter<'_> {
    fn drop(&mut self) {
        if let Some(p) = self.pending {
            self.buf.push(p | 0xF0);
        }
    }
}

pub struct NibbleReader<'a, 'b> {
    buf: &'b mut &'a [u8],
    pending: Option<u8>,
}

impl<'a, 'b> NibbleReader<'a, 'b> {
    pub fn new(buf: &'b mut &'a [u8]) -> Self {
        Self { buf, pending: None }
    }

    pub fn has_remaining(&self) -> bool {
        !self.buf.is_empty() || self.pending.is_some_and(|p| p != 15)
    }

    fn read_u4(&mut self) -> Option<u8> {
        if let Some(p) = self.pending.take() {
            Some(p)
        } else {
            let n = *self.buf.first()?;
            *self.buf = &self.buf[1..];
            self.pending = Some(n >> 4);
            Some(n & 15)
        }
    }

    pub fn read_u3(&mut self) -> Option<u8> {
        self.read_u4().filter(|&n| n < 8)
    }

    pub fn read_u32_varint(&mut self) -> Option<u32> {
        let mut n = 0;
        let mut shift = 0;
        loop {
            let b = self.read_u4()?;
            if shift == 30 && b & 4 != 0 {
                return None;
            }
            n |= ((b & 7) as u32) << shift;
            if b & 8 == 0 {
                return Some(n);
            }
            shift += 3;
            if shift >= 32 {
                return None;
            }
        }
    }
}
