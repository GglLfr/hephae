use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout, LayoutError},
    cell::UnsafeCell,
    convert::identity,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::{addr_eq, null_mut, slice_from_raw_parts_mut, NonNull},
};

use crossbeam_utils::{Backoff, CachePadded};

use crate::{likely, sync::*, unlikely, LikelyResult};

const FLAG: usize = 1 << (usize::BITS - 1);
const MASK: usize = !FLAG;

#[repr(C)]
struct Fragment<T> {
    next: *mut Self,
    len: usize,
    data: [MaybeUninit<T>],
}

impl<T> Fragment<T> {
    #[inline]
    fn layout(capacity: usize) -> Result<Layout, LayoutError> {
        let (layout, ..) = Layout::new::<*mut Self>().extend(Layout::new::<usize>())?;
        let (layout, ..) = layout.extend(Layout::array::<MaybeUninit<T>>(capacity)?)?;
        Ok(layout.pad_to_align())
    }

    #[inline]
    fn new(capacity: usize) -> Result<(NonNull<Self>, *mut T), LayoutError> {
        let layout = Self::layout(capacity)?;
        let ptr = slice_from_raw_parts_mut(unsafe { alloc(layout) } as *mut (), capacity) as *mut Self;
        if ptr.is_null() {
            handle_alloc_error(layout)
        }

        unsafe {
            (&raw mut (*ptr).next).write(slice_from_raw_parts_mut(null_mut::<()>(), 0) as *mut Self);
            (&raw mut (*ptr).len).write(0);
        }

        Ok(unsafe { (NonNull::new_unchecked(ptr), &raw mut (*ptr).data as *mut T) })
    }
}

pub struct VecBelt<T> {
    preceeding_len: UnsafeCell<usize>,
    len: CachePadded<AtomicUsize>,
    head: NonNull<Fragment<T>>,
    tail: UnsafeCell<NonNull<Fragment<T>>>,
}

unsafe impl<T: Send> Send for VecBelt<T> {}
unsafe impl<T: Send> Sync for VecBelt<T> {}

impl<T> VecBelt<T> {
    #[inline]
    pub fn new(initial_size: usize) -> Self {
        let (head, ..) = Fragment::new(initial_size).expect("couldn't allocate a fragment");
        Self {
            preceeding_len: UnsafeCell::new(0),
            len: CachePadded::new(AtomicUsize::new(0)),
            head,
            tail: UnsafeCell::new(head),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len.load(Relaxed)
    }

    #[inline]
    pub fn len_mut(&mut self) -> usize {
        *self.len.get_mut()
    }

    unsafe fn append_raw_erased(&self, additional: usize) -> (*mut T, usize) {
        let backoff = Backoff::new();
        let preceeding_len = self.preceeding_len.get();
        let tail = self.tail.get();

        loop {
            let len = self.len.load(Relaxed) & MASK;
            let new_len = len + additional;

            if unlikely(new_len & FLAG == FLAG) {
                panic!("too many elements")
            }

            if self.len.compare_exchange(len, new_len | FLAG, Acquire, Relaxed).is_err() {
                backoff.snooze();
                continue
            }

            let preceeding = *preceeding_len;
            let data = &raw mut (*(*tail).as_ptr()).data;

            break if likely(data.len() >= new_len - preceeding) {
                self.len.store(new_len, Release);
                ((data as *mut T).add(len - preceeding), len)
            } else {
                #[cold]
                unsafe fn new_fragment<T>(
                    len: usize,
                    additional: usize,
                    preceeding: usize,
                    this_preceeding_len: *mut usize,
                    this_len: &AtomicUsize,
                    this_tail: *mut NonNull<Fragment<T>>,
                ) -> (*mut T, usize) {
                    let (new_tail, new_data) = Fragment::<T>::new(len + additional).likely_ok(identity, |_| {
                        this_len.store(len, Release);
                        panic!("couldn't allocate a fragment");
                    });

                    this_preceeding_len.replace(len);
                    let tail = this_tail.replace(new_tail).as_ptr();
                    this_len.store(len + additional, Release);

                    (&raw mut (*tail).next).write(new_tail.as_ptr());
                    (&raw mut (*tail).len).write(len - preceeding);

                    (new_data as *mut T, len)
                }

                new_fragment(len, additional, preceeding, preceeding_len, &self.len, tail)
            }
        }
    }

    #[inline]
    pub unsafe fn append_raw(&self, additional: usize, acceptor: impl FnOnce(*mut T)) -> usize {
        let (data, index) = self.append_raw_erased(additional);

        acceptor(data);
        index
    }

    #[inline]
    pub fn append(&self, transfer: impl TransferBelt<T>) -> usize {
        let len = transfer.len();
        if unlikely(len & FLAG == FLAG) {
            panic!("too many elements")
        }

        unsafe { self.append_raw(len, |ptr| transfer.transfer(len, ptr)) }
    }

    #[inline]
    pub fn clear<'a, R>(&'a mut self, consumer: impl FnOnce(ConsumeSlice<'a, T>) -> R) -> R {
        #[cold]
        unsafe fn merge<T>(
            head: &mut NonNull<Fragment<T>>,
            tail: &mut NonNull<Fragment<T>>,
            preceeding: usize,
            current: usize,
        ) -> *mut [T] {
            let (new_head, mut new_data) =
                Fragment::<T>::new(current).likely_ok(identity, |_| panic!("couldn't allocate a fragment"));

            let start_data = new_data;
            let mut node = std::mem::replace(head, new_head).as_ptr();

            loop {
                let next = (*node).next;
                let data = &raw const (*node).data;
                let len = if next.is_null() { current - preceeding } else { (*node).len };

                new_data.copy_from_nonoverlapping(data as *const T, len);
                new_data = new_data.add(len);
                dealloc(node as *mut u8, Fragment::<T>::layout(data.len()).unwrap_unchecked());

                node = match next {
                    ptr if ptr.is_null() => {
                        *tail = new_head;
                        break slice_from_raw_parts_mut(start_data as *mut T, current)
                    }
                    ptr => ptr,
                };
            }
        }

        let preceeding = std::mem::replace(self.preceeding_len.get_mut(), 0);
        let current = std::mem::replace(self.len.get_mut(), 0);

        let head = self.head.as_ptr();
        let slice = if likely(addr_eq(head, self.tail.get_mut().as_ptr())) {
            slice_from_raw_parts_mut(unsafe { &raw mut (*head).data as *mut T }, current)
        } else {
            unsafe { merge(&mut self.head, self.tail.get_mut(), preceeding, current) }
        };

        consumer(ConsumeSlice {
            slice,
            _marker: PhantomData,
        })
    }
}

impl<T> Drop for VecBelt<T> {
    fn drop(&mut self) {
        let preceeding = *self.preceeding_len.get_mut();
        let current = *self.len.get_mut();
        let mut node = self.head.as_ptr();

        loop {
            let next = unsafe { (*node).next };
            let data = unsafe { &raw mut (*node).data };
            let taken = if next.is_null() {
                current - preceeding
            } else {
                unsafe { (*node).len }
            };

            unsafe {
                slice_from_raw_parts_mut(data as *mut T, taken).drop_in_place();
                dealloc(node as *mut u8, Fragment::<T>::layout(data.len()).unwrap_unchecked());
            }

            node = match next {
                ptr if ptr.is_null() => break,
                ptr => ptr,
            };
        }
    }
}

pub unsafe trait TransferBelt<T> {
    fn len(&self) -> usize;

    unsafe fn transfer(self, len: usize, dst: *mut T);
}

unsafe impl<T, const LEN: usize> TransferBelt<T> for [T; LEN] {
    #[inline]
    fn len(&self) -> usize {
        LEN
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_ptr() as *const T, len);
        std::mem::forget(self);
    }
}

unsafe impl<T, const LEN: usize> TransferBelt<T> for std::array::IntoIter<T, LEN> {
    #[inline]
    fn len(&self) -> usize {
        LEN
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_slice().as_ptr() as *const T, len);
        self.for_each(std::mem::forget);
    }
}

unsafe impl<T> TransferBelt<T> for Box<[T]> {
    #[inline]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        let ptr = Box::into_raw(self);
        dst.copy_from_nonoverlapping(ptr as *const T, len);

        drop(Box::from_raw(ptr as *mut MaybeUninit<T>));
    }
}

unsafe impl<T> TransferBelt<T> for Vec<T> {
    #[inline]
    fn len(&self) -> usize {
        <Vec<T>>::len(self)
    }

    #[inline]
    unsafe fn transfer(mut self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_ptr(), len);
        self.set_len(0);
    }
}

unsafe impl<T> TransferBelt<T> for std::vec::IntoIter<T> {
    #[inline]
    fn len(&self) -> usize {
        <Self as ExactSizeIterator>::len(self)
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_slice().as_ptr(), len);
        self.for_each(std::mem::forget);
    }
}

unsafe impl<T> TransferBelt<T> for std::vec::Drain<'_, T> {
    #[inline]
    fn len(&self) -> usize {
        <Self as ExactSizeIterator>::len(self)
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_slice().as_ptr(), len);
        self.for_each(std::mem::forget);
    }
}

unsafe impl<T: Copy> TransferBelt<T> for &[T] {
    #[inline]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_ptr(), len);
    }
}

unsafe impl<T: Copy> TransferBelt<T> for std::slice::Iter<'_, T> {
    #[inline]
    fn len(&self) -> usize {
        <Self as ExactSizeIterator>::len(self)
    }

    #[inline]
    unsafe fn transfer(self, len: usize, dst: *mut T) {
        dst.copy_from_nonoverlapping(self.as_slice().as_ptr(), len);
    }
}

pub struct ConsumeSlice<'a, T> {
    slice: *mut [T],
    _marker: PhantomData<&'a mut [T]>,
}

impl<T> Drop for ConsumeSlice<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { self.slice.drop_in_place() }
    }
}

impl<T> Deref for ConsumeSlice<'_, T> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.slice }
    }
}

impl<T> DerefMut for ConsumeSlice<'_, T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.slice }
    }
}

impl<'a, T> IntoIterator for ConsumeSlice<'a, T> {
    type Item = T;
    type IntoIter = ConsumeIter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        let this = ManuallyDrop::new(self);

        let len = this.slice.len();
        let begin = this.slice as *mut T;
        let end = unsafe { begin.add(len) };

        ConsumeIter {
            begin: unsafe { NonNull::new_unchecked(begin) },
            end: unsafe { NonNull::new_unchecked(end) },
            _marker: PhantomData,
        }
    }
}

pub struct ConsumeIter<'a, T> {
    begin: NonNull<T>,
    end: NonNull<T>,
    _marker: PhantomData<&'a mut [T]>,
}

impl<'a, T> ConsumeIter<'a, T> {
    #[inline]
    pub const fn slice_len(&self) -> usize {
        unsafe { self.end.offset_from(self.begin) as usize }
    }
}

impl<T> Iterator for ConsumeIter<'_, T> {
    type Item = T;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.slice_len() == 0 {
            return None
        }

        unsafe {
            let item = self.begin.read();
            self.begin = self.begin.add(1);

            Some(item)
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.slice_len();
        (len, Some(len))
    }
}

impl<T> DoubleEndedIterator for ConsumeIter<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.slice_len() == 0 {
            return None
        }

        unsafe {
            self.end = self.end.sub(1);
            Some(self.end.read())
        }
    }
}

impl<T> ExactSizeIterator for ConsumeIter<'_, T> {
    #[inline]
    fn len(&self) -> usize {
        self.slice_len()
    }
}

impl<T> Drop for ConsumeIter<'_, T> {
    #[inline]
    fn drop(&mut self) {
        unsafe { slice_from_raw_parts_mut(self.begin.as_ptr(), self.end.offset_from(self.begin) as usize).drop_in_place() }
    }
}

impl<'a, T> IntoIterator for &'a ConsumeSlice<'a, T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T> IntoIterator for &'a mut ConsumeSlice<'a, T> {
    type Item = &'a mut T;
    type IntoIter = std::slice::IterMut<'a, T>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[cfg(test)]
mod tests {
    use std::iter::repeat_with;

    use crate::{sync::*, vec_belt::VecBelt};

    #[derive(Debug, PartialEq)]
    struct Int(usize);

    impl Drop for Int {
        fn drop(&mut self) {
            println!("Dropped {}", self.0);
        }
    }

    #[test]
    fn test_vec_belt() {
        let append = [0, 1, 2, 3, 4];
        let thread_count = 8;

        let vec = Arc::new(VecBelt::new(1));
        let threads = (0..thread_count)
            .zip(repeat_with(|| vec.clone()))
            .map(|(i, vec)| thread::spawn(move || vec.append(append.map(|num| Int(num + i * append.len())))))
            .collect::<Box<_>>();

        for thread in threads {
            thread.join().unwrap();
        }

        Arc::into_inner(vec).unwrap().clear(|slice| {
            assert_eq!(slice.len(), append.len() * thread_count);
            for i in 0..thread_count {
                let slice = &slice[i * append.len()..(i + 1) * append.len()];
                for (j, num) in slice.iter().enumerate() {
                    assert_eq!(num.0, append[j] + slice[0].0);
                }
            }
        })
    }
}
