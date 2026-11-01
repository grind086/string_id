use std::{marker::PhantomData, ptr::NonNull};

/// Buffer pages will be at least this many bytes.
const MIN_PAGE_SIZE: usize = 4 * 1024; // 128 kB

/// A growable string buffer that pre-allocates pages of memory to minimize allocations.
pub struct StringBuffer<'buf> {
    active_page: Option<ActivePage>,
    pages: Vec<NonNull<[u8]>>,
    _marker: PhantomData<fn(&'buf ())>,
}

impl<'buf> StringBuffer<'buf> {
    /// Creates a new, empty `StringBuffer`.
    ///
    /// The buffer will not allocate until it is written to.
    pub const fn new() -> Self {
        Self {
            active_page: None,
            pages: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// The total number of bytes that have been allocated to store interned strings.
    pub fn allocated_bytes(&self) -> usize {
        self.pages.iter().map(|page| page.len()).sum()
    }

    /// The number of bytes that can be written without allocating.
    pub fn reserved_bytes(&self) -> usize {
        self.active_page
            .as_ref()
            .map_or(0, ActivePage::remaining_bytes)
    }

    /// Writes `s` into the buffer, and returns a reference to the buffered string.
    pub fn write<'a>(&'a mut self, s: &str) -> &'buf str {
        // NB: `s.len()` cannot be greater than `isize::MAX`, since that is the maximum number of
        // elements that can be in a `str`'s backing `Vec`.
        let buf = self.get_buffer_slice(s.len());
        buf.copy_from_slice(s.as_bytes());
        // SAFETY: We just wrote UTF-8 bytes into the buffer
        let buf_str = unsafe { str::from_utf8_unchecked(buf) };
        // SAFETY: The string lives in the buffer, so we can extend its lifetime to `'buf`.
        unsafe { core::mem::transmute::<&'a str, &'buf str>(buf_str) }
    }

    /// Retrieves the next `size` bytes in the buffer, allocating a new page if necessary.
    ///
    /// # Panics
    ///
    /// This function will panic if `req_bytes` is greater than [`isize::MAX`]. See
    /// [`Self::allocate_page`].
    fn get_buffer_slice(&mut self, size: usize) -> &mut [u8] {
        // We need to allocate a new page if there is no active page, or the active page doesn't
        // have enough space.
        if self
            .active_page
            .as_ref()
            .is_none_or(|page| page.remaining_bytes() < size)
        {
            core::hint::cold_path();
            self.allocate_page(size);
        }

        unsafe {
            self.active_page
                .as_mut()
                // SAFETY: We have a page allocated at this point
                .unwrap_unchecked()
                // SAFETY: We ensured that the page has at least `size` bytes left
                .take_slice(size)
        }
    }

    /// Allocates a page with at least `req_bytes` bytes.
    ///
    /// # Panics
    ///
    /// This function will panic if `req_bytes` is greater than [`isize::MAX`]. Note that [`Vec`]
    /// and [`String`] have this same length restriction, and should never have lengths that would
    /// cause a panic.
    fn allocate_page(&mut self, req_bytes: usize) {
        let alloc_bytes = req_bytes
            .checked_mul(2)
            .and_then(usize::checked_next_power_of_two)
            // `Vec` cannot exceed `isize::MAX` bytes
            .filter(|n| *n <= (isize::MAX as usize))
            .unwrap_or(req_bytes)
            .max(MIN_PAGE_SIZE);

        // NB: This optimizes to the same thing as using `alloc_zeroed` directly
        let bytes = vec![0u8; alloc_bytes].into_boxed_slice();
        let ptr = NonNull::new(Box::into_raw(bytes)).unwrap();
        self.active_page = Some(ActivePage { ptr });
        self.pages.push(ptr);
    }
}

impl Default for StringBuffer<'_> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: The buffer's pointers can be sent across threads safely.
unsafe impl Send for StringBuffer<'_> {}

impl Drop for StringBuffer<'_> {
    fn drop(&mut self) {
        for page in &mut self.pages {
            // SAFETY: The page pointer was created from `Box::into_raw`, and we have exclusive
            // mutable access.
            let _ = unsafe { Box::from_raw(page.as_ptr()) };
        }
    }
}

/// The active buffer page.
///
/// The internal pointer of this struct points into the active page's data, and allows successive
/// subslices to be fetched mutably exactly once.
struct ActivePage {
    ptr: NonNull<[u8]>,
}

impl ActivePage {
    /// The number of bytes remaining in the active page.
    fn remaining_bytes(&self) -> usize {
        self.ptr.len()
    }

    /// Returns the next `len` bytes in the page, and removes them from the raw slice in
    /// [`ActivePage::ptr`].
    ///
    /// # Safety
    ///
    /// The caller guarantees that there are at least `len` bytes remaining in the page.
    // NB: Making this unsafe and using `split_at_mut_unchecked` instead of `split_at` is a
    // significant performance gain.
    unsafe fn take_slice(&mut self, len: usize) -> &mut [u8] {
        // SAFETY: `ptr` is always valid, and we have exclusive access to its slice.
        let buffer = unsafe { &mut *self.ptr.as_ptr() };
        // SAFETY: Guarantee upheld by caller.
        let (slice, buffer) = unsafe { buffer.split_at_mut_unchecked(len) };
        self.ptr = NonNull::from_mut(buffer);
        slice
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fast allocation of a string composed of a repeated ascii character. This reduces miri test
    /// times for the allocation tests below from several minutes to around 1.5 seconds.
    fn alloc_string(c: u8, len: usize) -> String {
        vec![c; len].try_into().unwrap()
    }

    #[test]
    fn allocate_double_next_pow_2() {
        let mut buf = StringBuffer::new();
        assert_eq!(buf.allocated_bytes(), 0);
        assert_eq!(buf.reserved_bytes(), 0);

        let s = alloc_string(b'a', MIN_PAGE_SIZE);
        buf.write(&s);

        assert_eq!(
            buf.allocated_bytes(),
            (2 * MIN_PAGE_SIZE).next_power_of_two()
        );
        assert_eq!(
            buf.reserved_bytes(),
            (2 * MIN_PAGE_SIZE).next_power_of_two() - MIN_PAGE_SIZE
        );
    }

    #[test]
    fn allocate_at_least_min_page_size() {
        let mut buf = StringBuffer::new();
        assert_eq!(buf.allocated_bytes(), 0);
        assert_eq!(buf.reserved_bytes(), 0);

        // Force a `MIN_PAGE_SIZE` page to be allocated
        buf.write("a");
        assert_eq!(buf.allocated_bytes(), MIN_PAGE_SIZE);
        assert_eq!(buf.reserved_bytes(), MIN_PAGE_SIZE - 1);

        // Fill the rest of the buffer
        let s = alloc_string(b'a', MIN_PAGE_SIZE - 1);
        buf.write(&s);

        assert_eq!(buf.allocated_bytes(), MIN_PAGE_SIZE);
        assert_eq!(buf.reserved_bytes(), 0);

        // Allocate a second page
        buf.write("b");
        assert_eq!(buf.allocated_bytes(), 2 * MIN_PAGE_SIZE);
        assert_eq!(buf.reserved_bytes(), MIN_PAGE_SIZE - 1);
    }
}
