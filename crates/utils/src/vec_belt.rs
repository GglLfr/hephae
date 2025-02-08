use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout, LayoutError},
    cell::UnsafeCell,
    hint::assert_unchecked,
    marker::PhantomData,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr::{addr_eq, null_mut, slice_from_raw_parts_mut, NonNull},
};

use crossbeam_utils::{Backoff, CachePadded};

use crate::sync::*;

const FLAG: usize = 1 << (usize::BITS - 1);
const MASK: usize = !FLAG;

#[repr(C)]
struct Fragment<T> {
    next: *mut Self,
    data: [MaybeUninit<T>],
}

impl<T> Fragment<T> {
    #[inline]
    fn layout(capacity: usize) -> Result<Layout, LayoutError> {
        let (layout, ..) = Layout::new::<*mut Self>().extend(Layout::array::<MaybeUninit<T>>(capacity)?)?;
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
        }

        Ok(unsafe { (NonNull::new_unchecked(ptr), &raw mut (*ptr).data as *mut T) })
    }
}

pub struct VecBelt<T> {
    chunk_len: usize,
    total_len: UnsafeCell<usize>,
    len: CachePadded<AtomicUsize>,
    head: NonNull<Fragment<T>>,
    tail: UnsafeCell<NonNull<Fragment<T>>>,
}

unsafe impl<T: Send> Send for VecBelt<T> {}
unsafe impl<T: Send> Sync for VecBelt<T> {}

impl<T> VecBelt<T> {
    #[inline]
    pub fn new(chunk_len: usize) -> Self {
        let (head, ..) = Fragment::new(chunk_len).expect("couldn't allocate a fragment");
        Self {
            chunk_len,
            total_len: UnsafeCell::new(0),
            len: CachePadded::new(AtomicUsize::new(0)),
            head,
            tail: UnsafeCell::new(head),
        }
    }

    #[inline]
    pub fn len(&mut self) -> usize {
        *self.total_len.get_mut()
    }

    fn append_raw_erased(&self, additional: usize) -> (*mut [T], *mut T, usize) {
        let backoff = Backoff::new();

        let chunk_len = self.chunk_len;
        let total_len = self.total_len.get();
        let tail = self.tail.get();

        //let mut len_flagged = self.len.load(Relaxed);
        loop {
            //let len = len_flagged & MASK;
            let len = self.len.load(Relaxed) & MASK;
            let new_len = (len as isize).checked_add(additional as isize).expect("too many elements") as usize;

            if self.len.compare_exchange(len, new_len | FLAG, Acquire, Relaxed).is_err() {
                backoff.snooze();
                continue
            }

            /*if let Err(actual) = self.len.compare_exchange_weak(len, new_len | FLAG, Acquire, Relaxed) {
                backoff.snooze();
                len_flagged = actual;
                continue
            }*/

            let index = unsafe {
                total_len.replace(match (*total_len).checked_add(additional) {
                    Some(new_total_len) => new_total_len,
                    None => {
                        self.len.store(len, Release);
                        panic!("too many elements")
                    }
                })
            };

            let data = unsafe { &raw mut (*(*tail).as_ptr()).data } as *mut [T];
            break if data.len() >= new_len {
                self.len.store(new_len, Release);
                (
                    unsafe { slice_from_raw_parts_mut((data as *mut T).add(len), new_len - len) },
                    null_mut(),
                    index,
                )
            } else {
                #[cold]
                unsafe fn new_fragment<'a, T>(
                    len: usize,
                    new_len: usize,
                    chunk_len: usize,
                    this_len: &AtomicUsize,
                    this_tail: *mut NonNull<Fragment<T>>,
                    index: usize,
                ) -> (*mut [T], *mut T, usize) {
                    let tail = unsafe { (*this_tail).as_ptr() };
                    let data = unsafe { &raw mut (*tail).data } as *mut [T];

                    let cut = new_len - data.len();
                    let (new_tail, new_data) = match Fragment::<T>::new(chunk_len.max(index + (new_len - len)).max(cut)) {
                        Ok(frag) => frag,
                        Err(..) => {
                            this_len.store(len, Release);
                            panic!("couldn't allocate a fragment")
                        }
                    };

                    this_tail.write(new_tail);
                    this_len.store(cut, Release);

                    (&raw mut (*tail).next).write(new_tail.as_ptr());
                    (
                        slice_from_raw_parts_mut((data as *mut T).add(len), data.len() - len),
                        new_data,
                        index,
                    )
                }

                unsafe { new_fragment(len, new_len, chunk_len, &self.len, tail, index) }
            }
        }
    }

    #[inline]
    pub unsafe fn append_raw(&self, additional: usize, acceptor: impl FnOnce(*mut [T], *mut T)) -> usize {
        let (left, right, index) = self.append_raw_erased(additional);

        acceptor(left, right);
        index
    }

    #[inline]
    pub fn append(&self, transfer: impl TransferBelt<Item = T>) -> usize {
        let len = transfer.len();
        unsafe {
            self.append_raw(len, move |left, right| {
                let fit = left.len();
                transfer.transfer(0, fit, left as *mut T);
                if !right.is_null() {
                    transfer.transfer(fit, len - fit, right);
                }

                transfer.finish();
            })
        }
    }

    #[inline]
    pub fn clear<'a, R>(&'a mut self, consumer: impl FnOnce(ConsumeSlice<'a, T>) -> R) -> R {
        #[cold]
        #[inline(never)]
        fn merge<T>(head: *mut Fragment<T>, total_len: usize) -> (NonNull<Fragment<T>>, *mut [T]) {
            let (new_head, mut new_data) = Fragment::<T>::new(total_len).expect("couldn't allocate a fragment");

            let start_data = new_data;
            let mut node = head;
            let mut remaining = total_len;

            while !node.is_null() {
                let next = unsafe { (*node).next };

                let data = unsafe { &raw mut (*node).data };
                let capacity = data.len();
                let taken = remaining.min(capacity);

                unsafe {
                    new_data.copy_from_nonoverlapping(data as *const T, taken);
                    new_data = new_data.add(taken);

                    dealloc(node as *mut u8, Fragment::<T>::layout(capacity).unwrap_unchecked());
                }

                remaining -= taken;
                node = next;
            }

            unsafe { assert_unchecked(remaining == 0) }
            (new_head, slice_from_raw_parts_mut(start_data, total_len))
        }

        let total_len = std::mem::replace(self.total_len.get_mut(), 0);

        let head = self.head.as_ptr();
        let slice = if addr_eq(head, self.tail.get_mut().as_ptr()) {
            slice_from_raw_parts_mut(unsafe { &raw mut (*head).data as *mut T }, total_len)
        } else {
            let (new_head, slice) = merge(head, total_len);

            self.head = new_head;
            *self.tail.get_mut() = new_head;

            slice
        };

        consumer(ConsumeSlice {
            slice,
            _marker: PhantomData,
        })
    }
}

impl<T> Drop for VecBelt<T> {
    fn drop(&mut self) {
        let mut node = self.head.as_ptr();
        let mut remaining = *self.total_len.get_mut();

        while !node.is_null() {
            let next = unsafe { (*node).next };

            let data = unsafe { &raw mut (*node).data };
            let capacity = data.len();
            let taken = remaining.min(capacity);

            unsafe {
                slice_from_raw_parts_mut(data as *mut T, taken).drop_in_place();
                dealloc(node as *mut u8, Fragment::<T>::layout(capacity).unwrap_unchecked());
            }

            remaining -= taken;
            node = next;
        }

        unsafe { assert_unchecked(remaining == 0) }
    }
}

pub unsafe trait TransferBelt {
    type Item;

    fn len(&self) -> usize;

    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item);

    unsafe fn finish(self);
}

unsafe impl<T, const LEN: usize> TransferBelt for [T; LEN] {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        LEN
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {
        std::mem::forget(self);
    }
}

unsafe impl<T, const LEN: usize> TransferBelt for std::array::IntoIter<T, LEN> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        LEN
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_slice().as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {
        self.for_each(std::mem::forget);
    }
}

unsafe impl<T> TransferBelt for Box<[T]> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {
        drop(Box::from_raw(Box::into_raw(self) as *mut [MaybeUninit<T>]));
    }
}

unsafe impl<T> TransferBelt for Vec<T> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        <Vec<T>>::len(self)
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(mut self) {
        self.set_len(0);
    }
}

unsafe impl<T> TransferBelt for std::vec::IntoIter<T> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_slice().as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {
        self.for_each(std::mem::forget);
    }
}

unsafe impl<T: Copy> TransferBelt for &[T] {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {}
}

unsafe impl<T: Copy> TransferBelt for &mut [T] {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        <[T]>::len(self)
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {}
}

unsafe impl<T: Copy> TransferBelt for std::slice::Iter<'_, T> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_slice().as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {}
}

unsafe impl<T: Copy> TransferBelt for std::slice::IterMut<'_, T> {
    type Item = T;

    #[inline]
    fn len(&self) -> usize {
        self.as_slice().len()
    }

    #[inline]
    unsafe fn transfer(&self, offset: usize, len: usize, dst: *mut Self::Item) {
        let ptr = self.as_slice().as_ptr();
        dst.copy_from_nonoverlapping(ptr.add(offset), len);
    }

    #[inline]
    unsafe fn finish(self) {}
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

    #[test]
    fn test_vec_belt() {
        let append = [0, 1, 2, 3, 4];
        let thread_count = 8;

        let vec = Arc::new(VecBelt::new(1));
        let threads = (0..thread_count)
            .zip(repeat_with(|| vec.clone()))
            .map(|(i, vec)| thread::spawn(move || vec.append(append.map(|num| num + i * append.len()))))
            .collect::<Box<_>>();

        for thread in threads {
            thread.join().unwrap();
        }

        Arc::into_inner(vec).unwrap().clear(|slice| {
            assert_eq!(slice.len(), append.len() * thread_count);
            for i in 0..thread_count {
                let slice = &slice[i * append.len()..(i + 1) * append.len()];
                assert_eq!(slice, append.map(|num| num + slice[0]));
            }
        })
    }
}
