pub struct BitWriter<'a> {
    buf: &'a mut Vec<u8>,
    pending: u16,
    pending_bits: u8,
}

impl<'a> BitWriter<'a> {
    pub fn new(buf: &'a mut Vec<u8>) -> Self {
        Self {
            buf,
            pending: 0,
            pending_bits: 0,
        }
    }

    pub fn write(&mut self, n: u8, n_bits: u8) {
        assert!((1..=8).contains(&n_bits));
        let mask = ((1u16 << n_bits) - 1) as u8;
        let n = n & mask;
        self.pending |= (n as u16) << self.pending_bits;
        self.pending_bits += n_bits;
        if self.pending_bits >= 8 {
            self.buf.push(self.pending as u8);
            self.pending >>= 8;
            self.pending_bits -= 8;
        }
    }

    pub fn write_u32_varint(&mut self, mut n: u32, n_bits: u8) {
        assert!((2..=8).contains(&n_bits));
        let payload_bits = n_bits - 1;
        let continuation_bit = 1 << payload_bits;
        let mask = continuation_bit - 1;

        while n >= continuation_bit {
            self.write((n as u8 & mask as u8) | continuation_bit as u8, n_bits);
            n >>= payload_bits;
        }
        self.write(n as u8, n_bits);
    }
}

impl Drop for BitWriter<'_> {
    fn drop(&mut self) {
        if self.pending_bits > 0 {
            let mask = 0xFFu16 << self.pending_bits;
            self.buf.push((self.pending | mask) as u8);
        }
    }
}

pub struct BitReader<'a, 'b> {
    buf: &'b mut &'a [u8],
    pending: u16,
    pending_bits: u8,
}

impl<'a, 'b> BitReader<'a, 'b> {
    pub fn new(buf: &'b mut &'a [u8]) -> Self {
        Self {
            buf,
            pending: 0,
            pending_bits: 0,
        }
    }

    pub fn has_remaining(&self) -> bool {
        if !self.buf.is_empty() {
            return true;
        }
        if self.pending_bits == 0 {
            return false;
        }
        let mask = (1u16 << self.pending_bits) - 1;
        (self.pending & mask) != mask
    }

    pub fn read(&mut self, n_bits: u8) -> Option<u8> {
        assert!((1..=8).contains(&n_bits));
        if self.pending_bits < n_bits {
            if self.buf.is_empty() {
                return None;
            }
            let b = self.buf[0];
            *self.buf = &self.buf[1..];
            self.pending |= (b as u16) << self.pending_bits;
            self.pending_bits += 8;
        }

        let mask = (1u16 << n_bits) - 1;
        let res = (self.pending & mask) as u8;
        self.pending >>= n_bits;
        self.pending_bits -= n_bits;
        Some(res)
    }

    pub fn read_u32_varint(&mut self, n_bits: u8) -> Option<u32> {
        assert!((2..=8).contains(&n_bits));
        let payload_bits = n_bits - 1;
        let continuation_bit = 1 << payload_bits;
        let mask = continuation_bit - 1;

        let mut n = 0;
        let mut shift = 0;
        loop {
            let b = self.read(n_bits)?;
            if shift + payload_bits > 32 {
                let allowed = 32 - shift;
                if (b & mask as u8) >= (1 << allowed) {
                    return None;
                }
            }
            n |= ((b & mask as u8) as u32) << shift;
            if b & continuation_bit as u8 == 0 {
                return Some(n);
            }
            shift += payload_bits;
            if shift >= 32 {
                return None;
            }
        }
    }
}
