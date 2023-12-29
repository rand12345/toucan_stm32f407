pub struct ByteMutWriter<'a> {
    pub buf: &'a mut [u8],
    pub cursor: usize,
}

impl<'a> ByteMutWriter<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        ByteMutWriter { buf, cursor: 0 }
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    // pub fn as_str(&self) -> &str {
    //     use core::str;
    //     str::from_utf8(&self.buf[0..self.cursor]).unwrap()
    // }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.buf.len()
    }

    pub fn clear(&mut self) {
        self.buf.fill(0);
        self.cursor = 0;
    }

    // pub fn len(&self) -> usize {
    //     self.cursor
    // }

    // pub fn empty(&self) -> bool {
    //     self.cursor == 0
    // }

    // pub fn full(&self) -> bool {
    //     self.capacity() == self.cursor
    // }
}

impl core::fmt::Write for ByteMutWriter<'_> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let cap = self.capacity();
        for (i, &b) in self.buf[self.cursor..cap]
            .iter_mut()
            .zip(s.as_bytes().iter())
        {
            *i = b;
        }
        self.cursor = usize::min(cap, self.cursor + s.as_bytes().len());
        Ok(())
    }
}
