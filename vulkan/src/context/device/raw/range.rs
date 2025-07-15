use std::marker::PhantomData;

use bytemuck::AnyBitPattern;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct ByteRange {
    pub beg: usize,
    pub end: usize,
}

impl ByteRange {
    pub fn empty() -> Self {
        Self { beg: 0, end: 0 }
    }

    pub fn new(size: usize) -> Self {
        Self { beg: 0, end: size }
    }

    pub fn align<T>(offset: usize) -> usize {
        let alignment = std::mem::align_of::<T>();
        ((offset + alignment - 1) / alignment) * alignment
    }

    pub fn align_raw(offset: usize, alignment: usize) -> usize {
        ((offset + alignment - 1) / alignment) * alignment
    }

    pub fn extend<T: AnyBitPattern>(&mut self, len: usize) -> ByteRange {
        let beg = ByteRange::align::<T>(self.end);
        let end = beg + len * size_of::<T>();
        self.end = end;
        ByteRange { beg, end }
    }

    pub fn extend_raw(&mut self, len: usize, alignment: usize) -> ByteRange {
        let beg = ByteRange::align_raw(self.end, alignment);
        let end = beg + len;
        self.end = end;
        ByteRange { beg, end }
    }

    pub fn take<T: AnyBitPattern>(&mut self, count: usize) -> Option<ByteRange> {
        let beg = ByteRange::align::<T>(self.beg);
        let end = beg + count * size_of::<T>();
        if end <= self.end {
            self.beg = end;
            Some(ByteRange { beg, end })
        } else {
            None
        }
    }

    pub fn alloc_raw(&mut self, size: usize, alignment: usize) -> Option<ByteRange> {
        let beg = ByteRange::align_raw(self.beg, alignment);
        let end = beg + size;
        if end <= self.end {
            self.beg = end;
            Some(ByteRange { beg, end })
        } else {
            None
        }
    }

    pub fn len(&self) -> usize {
        self.end - self.beg
    }
}

impl<T: AnyBitPattern> From<Range<T>> for ByteRange {
    fn from(value: Range<T>) -> Self {
        let beg = value.first * size_of::<T>();
        Self {
            beg,
            end: beg + value.len * size_of::<T>(),
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct Range<T: AnyBitPattern> {
    pub len: usize,
    pub first: usize,
    pub _phantom: PhantomData<T>,
}

impl<T: AnyBitPattern> From<ByteRange> for Range<T> {
    fn from(value: ByteRange) -> Self {
        debug_assert_eq!(
            value.beg % size_of::<T>(),
            0,
            "Invalid Range<u8> offset for Range<{}> type!",
            std::any::type_name::<T>()
        );
        debug_assert_eq!(
            (value.end - value.beg) % size_of::<T>(),
            0,
            "Invalid Range<u8> size for Range<{}> type!",
            std::any::type_name::<T>()
        );
        Self {
            first: value.beg / size_of::<T>(),
            len: (value.end - value.beg) / size_of::<T>(),
            _phantom: PhantomData,
        }
    }
}

impl<T: AnyBitPattern> Range<T> {
    pub fn alloc(&mut self, len: usize) -> Self {
        debug_assert!(len <= self.len, "Range alloc overflow!");
        let first = self.first;
        self.first += len;
        self.len -= len;
        Self {
            first,
            len,
            _phantom: PhantomData,
        }
    }
}
