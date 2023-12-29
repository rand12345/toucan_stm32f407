// `OptionalCell` convenience type
// Tock specific `TakeCell` type for sharing references.

/// A shared reference to a mutable reference.
///
/// A `TakeCell` wraps potential reference to mutable memory that may be
/// available at a given point. Rather than enforcing borrow rules at
/// compile-time, `TakeCell` enables multiple clients to hold references to it,
/// but ensures that only one referrer has access to the underlying mutable
/// reference at a time. Clients either move the memory out of the `TakeCell` or
/// operate on a borrow within a closure. Attempts to take the value from inside
/// a `TakeCell` may fail by returning `None`.
///
///
///
// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

// Tock specific `MapCell` type for sharing references.
use core::cell::{Cell, UnsafeCell};
use core::mem::MaybeUninit;
use core::ptr::drop_in_place;

use core::ops::{Bound, Range, RangeBounds};
use core::ops::{Index, IndexMut};
use core::slice::SliceIndex;

/// A mutable leasable buffer implementation.
///
/// A leasable buffer can be used to pass a section of a larger mutable buffer
/// but still get the entire buffer back in a callback.
#[derive(Debug, PartialEq)]
pub struct SubSliceMut<'a, T> {
    internal: &'a mut [T],
    active_range: Range<usize>,
}

/// An immutable leasable buffer implementation.
///
/// A leasable buffer can be used to pass a section of a larger mutable buffer
/// but still get the entire buffer back in a callback.
#[derive(Debug, PartialEq)]
pub struct SubSlice<'a, T> {
    internal: &'a [T],
    active_range: Range<usize>,
}

/// Holder for either a mutable or immutable SubSlice.
///
/// In cases where code needs to support either a mutable or immutable SubSlice,
/// `SubSliceMutImmut` allows the code to store a single type which can
/// represent either option.
pub enum SubSliceMutImmut<'a, T> {
    Immutable(SubSlice<'a, T>),
    Mutable(SubSliceMut<'a, T>),
}

impl<'a, T> SubSliceMutImmut<'a, T> {
    pub fn reset(&mut self) {
        match *self {
            SubSliceMutImmut::Immutable(ref mut buf) => buf.reset(),
            SubSliceMutImmut::Mutable(ref mut buf) => buf.reset(),
        }
    }

    /// Returns the length of the currently accessible portion of the
    /// SubSlice.
    pub fn len(&self) -> usize {
        match *self {
            SubSliceMutImmut::Immutable(ref buf) => buf.len(),
            SubSliceMutImmut::Mutable(ref buf) => buf.len(),
        }
    }

    pub fn slice<R: RangeBounds<usize>>(&mut self, range: R) {
        match *self {
            SubSliceMutImmut::Immutable(ref mut buf) => buf.slice(range),
            SubSliceMutImmut::Mutable(ref mut buf) => buf.slice(range),
        }
    }
}

impl<'a, T, I> Index<I> for SubSliceMutImmut<'a, T>
where
    I: SliceIndex<[T]>,
{
    type Output = <I as SliceIndex<[T]>>::Output;

    fn index(&self, idx: I) -> &Self::Output {
        match *self {
            SubSliceMutImmut::Immutable(ref buf) => &buf[idx],
            SubSliceMutImmut::Mutable(ref buf) => &buf[idx],
        }
    }
}

impl<'a, T> SubSliceMut<'a, T> {
    /// Create a SubSlice from a passed reference to a raw buffer.
    pub fn new(buffer: &'a mut [T]) -> Self {
        let len = buffer.len();
        SubSliceMut {
            internal: buffer,
            active_range: 0..len,
        }
    }

    fn active_slice(&self) -> &[T] {
        &self.internal[self.active_range.clone()]
    }

    /// Retrieve the raw buffer used to create the SubSlice. Consumes the
    /// SubSlice.
    pub fn take(self) -> &'a mut [T] {
        self.internal
    }

    /// Resets the SubSlice to its full size, making the entire buffer
    /// accessible again.
    ///
    /// This should only be called by layer that created the SubSlice, and not
    /// layers that were passed a SubSlice. Layers which are using a SubSlice
    /// should treat the SubSlice as a traditional Rust slice and not consider
    /// any additional size to the underlying buffer.
    ///
    /// Most commonly, this is called once a sliced leasable buffer is returned
    /// through a callback.
    pub fn reset(&mut self) {
        self.active_range = 0..self.internal.len();
    }

    /// Returns the length of the currently accessible portion of the SubSlice.
    pub fn len(&self) -> usize {
        self.active_slice().len()
    }

    /// Returns a pointer to the currently accessible portion of the SubSlice.
    pub fn as_ptr(&self) -> *const T {
        self.active_slice().as_ptr()
    }

    /// Returns a slice of the currently accessible portion of the
    /// LeasableBuffer.
    pub fn as_slice(&mut self) -> &mut [T] {
        &mut self.internal[self.active_range.clone()]
    }

    /// Returns `true` if the LeasableBuffer is sliced internally.
    ///
    /// This is a useful check when switching between code that uses
    /// LeasableBuffers and code that uses traditional slice-and-length. Since
    /// slice-and-length _only_ supports using the entire buffer it is not valid
    /// to try to use a sliced LeasableBuffer.
    pub fn is_sliced(&self) -> bool {
        self.internal.len() != self.len()
    }

    /// Reduces the range of the SubSlice that is accessible.
    ///
    /// This should be called whenever a layer wishes to pass only a portion of
    /// a larger buffer to another layer.
    ///
    /// For example, if the application layer has a 1500 byte packet buffer, but
    /// wishes to send a 250 byte packet, the upper layer should slice the
    /// SubSlice down to its first 250 bytes before passing it down:
    ///
    /// ```rust,ignore
    /// let buffer = static_init!([u8; 1500], [0; 1500]);
    /// let s = SubSliceMut::new(buffer);
    /// s.slice(0..250);
    /// network.send(s);
    /// ```
    pub fn slice<R: RangeBounds<usize>>(&mut self, range: R) {
        let start = match range.start_bound() {
            Bound::Included(s) => *s,
            Bound::Excluded(s) => *s + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(e) => *e + 1,
            Bound::Excluded(e) => *e,
            Bound::Unbounded => self.active_range.end,
        };

        let new_start = self.active_range.start + start;
        let new_end = new_start + (end - start);

        self.active_range = Range {
            start: new_start,
            end: new_end,
        };
    }
}

impl<'a, T, I> Index<I> for SubSliceMut<'a, T>
where
    I: SliceIndex<[T]>,
{
    type Output = <I as SliceIndex<[T]>>::Output;

    fn index(&self, idx: I) -> &Self::Output {
        &self.internal[self.active_range.clone()][idx]
    }
}

impl<'a, T, I> IndexMut<I> for SubSliceMut<'a, T>
where
    I: SliceIndex<[T]>,
{
    fn index_mut(&mut self, idx: I) -> &mut Self::Output {
        &mut self.internal[self.active_range.clone()][idx]
    }
}

impl<'a, T> SubSlice<'a, T> {
    /// Create a SubSlice from a passed reference to a raw buffer.
    pub fn new(buffer: &'a [T]) -> Self {
        let len = buffer.len();
        SubSlice {
            internal: buffer,
            active_range: 0..len,
        }
    }

    fn active_slice(&self) -> &[T] {
        &self.internal[self.active_range.clone()]
    }

    /// Retrieve the raw buffer used to create the SubSlice. Consumes the
    /// SubSlice.
    pub fn take(self) -> &'a [T] {
        self.internal
    }

    /// Resets the SubSlice to its full size, making the entire buffer
    /// accessible again.
    ///
    /// This should only be called by layer that created the SubSlice, and not
    /// layers that were passed a SubSlice. Layers which are using a SubSlice
    /// should treat the SubSlice as a traditional Rust slice and not consider
    /// any additional size to the underlying buffer.
    ///
    /// Most commonly, this is called once a sliced leasable buffer is returned
    /// through a callback.
    pub fn reset(&mut self) {
        self.active_range = 0..self.internal.len();
    }

    /// Returns the length of the currently accessible portion of the SubSlice.
    pub fn len(&self) -> usize {
        self.active_slice().len()
    }

    /// Returns a pointer to the currently accessible portion of the SubSlice.
    pub fn as_ptr(&self) -> *const T {
        self.active_slice().as_ptr()
    }

    /// Returns a slice of the currently accessible portion of the
    /// LeasableBuffer.
    pub fn as_slice(&self) -> &[T] {
        &self.internal[self.active_range.clone()]
    }

    /// Returns `true` if the LeasableBuffer is sliced internally.
    ///
    /// This is a useful check when switching between code that uses
    /// LeasableBuffers and code that uses traditional slice-and-length. Since
    /// slice-and-length _only_ supports using the entire buffer it is not valid
    /// to try to use a sliced LeasableBuffer.
    pub fn is_sliced(&self) -> bool {
        self.internal.len() != self.len()
    }

    /// Reduces the range of the SubSlice that is accessible.
    ///
    /// This should be called whenever a layer wishes to pass only a portion of
    /// a larger buffer to another layer.
    ///
    /// For example, if the application layer has a 1500 byte packet buffer, but
    /// wishes to send a 250 byte packet, the upper layer should slice the
    /// SubSlice down to its first 250 bytes before passing it down:
    ///
    /// ```rust,ignore
    /// let buffer = unsafe {
    ///    core::slice::from_raw_parts(&_ptr_in_flash as *const u8, 1500)
    /// };
    /// let s = SubSlice::new(buffer);
    /// s.slice(0..250);
    /// network.send(s);
    /// ```
    pub fn slice<R: RangeBounds<usize>>(&mut self, range: R) {
        let start = match range.start_bound() {
            Bound::Included(s) => *s,
            Bound::Excluded(s) => *s + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(e) => *e + 1,
            Bound::Excluded(e) => *e,
            Bound::Unbounded => self.active_range.end,
        };

        let new_start = self.active_range.start + start;
        let new_end = new_start + (end - start);

        self.active_range = Range {
            start: new_start,
            end: new_end,
        };
    }
}

impl<'a, T, I> Index<I> for SubSlice<'a, T>
where
    I: SliceIndex<[T]>,
{
    type Output = <I as SliceIndex<[T]>>::Output;

    fn index(&self, idx: I) -> &Self::Output {
        &self.internal[self.active_range.clone()][idx]
    }
}

pub struct TakeCell<'a, T: 'a + ?Sized> {
    val: Cell<Option<&'a mut T>>,
}

impl<'a, T: ?Sized> TakeCell<'a, T> {
    pub fn empty() -> TakeCell<'a, T> {
        TakeCell {
            val: Cell::new(None),
        }
    }

    /// Creates a new `TakeCell` containing `value`
    pub fn new(value: &'a mut T) -> TakeCell<'a, T> {
        TakeCell {
            val: Cell::new(Some(value)),
        }
    }

    pub fn is_none(&self) -> bool {
        let inner = self.take();
        let return_val = inner.is_none();
        self.val.set(inner);
        return_val
    }

    pub fn is_some(&self) -> bool {
        !self.is_none()
    }

    /// Takes the mutable reference out of the `TakeCell` leaving a `None` in
    /// it's place. If the value has already been taken elsewhere (and not
    /// `replace`ed), the returned `Option` will be empty.
    ///
    /// # Examples
    ///
    /// ```
    /// extern crate tock_cells;
    /// use tock_cells::take_cell::TakeCell;
    ///
    /// let mut value = 1234;
    /// let cell = TakeCell::new(&mut value);
    /// let x = &cell;
    /// let y = &cell;
    ///
    /// x.take();
    /// assert_eq!(y.take(), None);
    /// ```
    pub fn take(&self) -> Option<&'a mut T> {
        self.val.replace(None)
    }

    /// Stores `val` in the `TakeCell`
    pub fn put(&self, val: Option<&'a mut T>) {
        self.val.replace(val);
    }

    /// Replaces the contents of the `TakeCell` with `val`. If the cell was not
    /// empty, the previous value is returned, otherwise `None` is returned.
    pub fn replace(&self, val: &'a mut T) -> Option<&'a mut T> {
        self.val.replace(Some(val))
    }

    /// Retrieves a mutable reference to the inner value that only lives as long
    /// as the reference to this does.
    ///
    /// This escapes the "take" aspect of TakeCell in a way which is guaranteed
    /// safe due to the returned reference sharing the lifetime of `&mut self`.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.val.get_mut().as_mut().map(|v| &mut **v)
    }

    /// Allows `closure` to borrow the contents of the `TakeCell` if-and-only-if
    /// it is not `take`n already. The state of the `TakeCell` is unchanged
    /// after the closure completes.
    ///
    /// # Examples
    ///
    /// ```
    /// extern crate tock_cells;
    /// use tock_cells::take_cell::TakeCell;
    ///
    /// let mut value = 1234;
    /// let cell = TakeCell::new(&mut value);
    /// let x = &cell;
    /// let y = &cell;
    ///
    /// x.map(|value| {
    ///     // We have mutable access to the value while in the closure
    ///     *value += 1;
    /// });
    ///
    /// // After the closure completes, the mutable memory is still in the cell,
    /// // but potentially changed.
    /// assert_eq!(y.take(), Some(&mut 1235));
    /// ```
    pub fn map<F, R>(&self, closure: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        let maybe_val = self.take();
        maybe_val.map(|mut val| {
            let res = closure(&mut val);
            self.replace(val);
            res
        })
    }

    /// Performs a `map` or returns a default value if the `TakeCell` is empty
    pub fn map_or<F, R>(&self, default: R, closure: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        let maybe_val = self.take();
        maybe_val.map_or(default, |mut val| {
            let res = closure(&mut val);
            self.replace(val);
            res
        })
    }

    /// Performs a `map` or generates a value with the default
    /// closure if the `TakeCell` is empty
    pub fn map_or_else<U, D, F>(&self, default: D, f: F) -> U
    where
        D: FnOnce() -> U,
        F: FnOnce(&mut T) -> U,
    {
        let maybe_val = self.take();
        maybe_val.map_or_else(
            || default(),
            |mut val| {
                let res = f(&mut val);
                self.replace(val);
                res
            },
        )
    }

    /// Behaves the same as `map`, except the closure is allowed to return
    /// an `Option`.
    pub fn and_then<F, R>(&self, closure: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> Option<R>,
    {
        let maybe_val = self.take();
        maybe_val.and_then(|mut val| {
            let res = closure(&mut val);
            self.replace(val);
            res
        })
    }

    /// Uses the first closure (`modify`) to modify the value in the `TakeCell`
    /// if it is present, otherwise, fills the `TakeCell` with the result of
    /// `mkval`.
    pub fn modify_or_replace<F, G>(&self, modify: F, mkval: G)
    where
        F: FnOnce(&mut T),
        G: FnOnce() -> &'a mut T,
    {
        let val = match self.take() {
            Some(mut val) => {
                modify(&mut val);
                val
            }
            None => mkval(),
        };
        self.replace(val);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum MapCellState {
    Uninit,
    Init,
    Borrowed,
}

#[inline(never)]
#[cold]
fn access_panic() {
    panic!("`MapCell` accessed while borrowed");
}

macro_rules! debug_assert_not_borrowed {
    ($slf:ident) => {
        if cfg!(debug_assertions) && $slf.occupied.get() == MapCellState::Borrowed {
            access_panic();
        }
    };
}

/// A mutable, possibly unset, memory location that provides checked `&mut` access
/// to its contents via a closure.
///
/// A `MapCell` provides checked shared access to its mutable memory. Borrow
/// rules are enforced by forcing clients to either move the memory out of the
/// cell or operate on a `&mut` within a closure. You can think of a `MapCell`
/// as a `Cell<Option<T>>` with an extra "in-use" state to prevent `map` from invoking
/// undefined behavior when called re-entrantly.
///
/// # Examples
/// ```
/// # use tock_cells::map_cell::MapCell;
/// let cell: MapCell<i64> = MapCell::empty();
///
/// assert!(cell.is_none());
/// cell.map(|_| unreachable!("The cell is empty; map does not call the closure"));
/// assert_eq!(cell.take(), None);
/// cell.put(10);
/// assert_eq!(cell.take(), Some(10));
/// assert_eq!(cell.replace(20), None);
/// assert_eq!(cell.get(), Some(20));
///
/// cell.map(|x| {
///     assert_eq!(x, &mut 20);
///     // `map` provides a `&mut` to the contents inside the closure
///     *x = 30;
/// });
/// assert_eq!(cell.replace(60), Some(30));
/// ```
pub struct MapCell<T> {
    // Since val is potentially uninitialized memory, we must be sure to check
    // `.occupied` before calling `.val.get()` or `.val.assume_init()`. See
    // [mem::MaybeUninit](https://doc.rust-lang.org/core/mem/union.MaybeUninit.html).
    val: UnsafeCell<MaybeUninit<T>>,

    // Safety invariants:
    // - The contents of `val` must be initialized if this is `Init` or `InsideMap`.
    // - It must be sound to mutate `val` behind a shared reference if this is `Uninit` or `Init`.
    //   No outside mutation can occur while a `&mut` to the contents of `val` exist.
    occupied: Cell<MapCellState>,
}

impl<T> Drop for MapCell<T> {
    fn drop(&mut self) {
        let state = self.occupied.get();
        debug_assert_not_borrowed!(self); // This should be impossible
        if state == MapCellState::Init {
            unsafe {
                // SAFETY:
                // - `occupied` is `Init`; `val` is initialized as an invariant.
                // - Even though this violates the `occupied` invariant, by causing `val`
                //   to be no longer valid, `self` is immediately dropped.
                drop_in_place(self.val.get_mut().as_mut_ptr())
            }
        }
    }
}

impl<T: Copy> MapCell<T> {
    /// Gets the contents of the cell, if any.
    ///
    /// Returns `None` if the cell is empty.
    ///
    /// This requires the held type be `Copy` for the same reason [`Cell::get`] does:
    /// it leaves the contents of `self` intact and so it can't have drop glue.
    ///
    /// This returns `None` in release mode if the `MapCell`'s contents are already borrowed.
    ///
    /// # Examples
    /// ```
    /// # use tock_cells::map_cell::MapCell;
    /// let cell: MapCell<u32> = MapCell::empty();
    /// assert_eq!(cell.get(), None);
    ///
    /// cell.put(20);
    /// assert_eq!(cell.get(), Some(20));
    /// ```
    ///
    /// # Panics
    /// If debug assertions are enabled, this panics if the `MapCell`'s contents are already borrowed.
    pub fn get(&self) -> Option<T> {
        debug_assert_not_borrowed!(self);
        // SAFETY:
        // - `Init` means that `val` is initialized and can be read
        // - `T: Copy` so there is no drop glue
        (self.occupied.get() == MapCellState::Init)
            .then(|| unsafe { self.val.get().read().assume_init() })
    }
}

impl<T> MapCell<T> {
    /// Creates an empty `MapCell`.
    pub const fn empty() -> MapCell<T> {
        MapCell {
            val: UnsafeCell::new(MaybeUninit::uninit()),
            occupied: Cell::new(MapCellState::Uninit),
        }
    }

    /// Creates a new `MapCell` containing `value`.
    pub const fn new(value: T) -> MapCell<T> {
        MapCell {
            val: UnsafeCell::new(MaybeUninit::new(value)),
            occupied: Cell::new(MapCellState::Init),
        }
    }

    /// Returns `true` if the `MapCell` contains no value.
    ///
    /// # Examples
    /// ```
    /// # use tock_cells::map_cell::MapCell;
    /// let x: MapCell<i32> = MapCell::empty();
    /// assert!(x.is_none());
    ///
    /// x.put(10);
    /// x.map(|_| assert!(!x.is_none()));
    /// assert!(!x.is_none());
    /// ```
    pub fn is_none(&self) -> bool {
        !self.is_some()
    }

    /// Returns `true` if the `MapCell` contains a value.
    ///
    /// # Examples
    /// ```
    /// # use tock_cells::map_cell::MapCell;
    /// let x: MapCell<i32> = MapCell::new(10);
    /// assert!(x.is_some());
    /// x.map(|_| assert!(x.is_some()));
    ///
    /// x.take();
    /// assert!(!x.is_some());
    /// ```
    pub fn is_some(&self) -> bool {
        self.occupied.get() != MapCellState::Uninit
    }

    /// Takes the value out of the `MapCell`, leaving it empty.
    ///
    /// Returns `None` if the cell is empty.
    ///
    /// To save size, this has no effect and returns `None` in release mode
    /// if the `MapCell`'s contents are already borrowed.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tock_cells::map_cell::MapCell;
    /// let cell = MapCell::new(1234);
    /// let x = &cell;
    /// let y = &cell;
    ///
    /// assert_eq!(x.take(), Some(1234));
    /// assert_eq!(y.take(), None);
    /// ```
    ///
    /// # Panics
    /// If debug assertions are enabled, this panics if the `MapCell`'s contents are already borrowed.
    pub fn take(&self) -> Option<T> {
        debug_assert_not_borrowed!(self);
        (self.occupied.get() == MapCellState::Init).then(|| {
            // SAFETY: Since `occupied` is `Init`, `val` is initialized and can be mutated
            //         behind a shared reference. `result` is therefore initialized.
            unsafe {
                let result: MaybeUninit<T> = self.val.get().replace(MaybeUninit::uninit());
                self.occupied.set(MapCellState::Uninit);
                result.assume_init()
            }
        })
    }

    /// Puts a value into the `MapCell` without returning the old value.
    ///
    /// To save size, this has no effect in release mode if `map` is invoking
    /// a closure for this cell.
    ///
    /// # Panics
    /// If debug assertions are enabled, this panics if the `MapCell`'s contents are already borrowed.
    pub fn put(&self, val: T) {
        debug_assert_not_borrowed!(self);
        // This will ensure the value as dropped
        self.replace(val);
    }

    /// Replaces the contents of the `MapCell`, returning the old value if available.
    ///
    /// To save size, this has no effect and returns `None` in release mode
    /// if the `MapCell`'s contents are already borrowed.
    ///
    /// # Panics
    /// If debug assertions are enabled, this panics if the `MapCell`'s contents are already borrowed.
    pub fn replace(&self, val: T) -> Option<T> {
        let occupied = self.occupied.get();
        debug_assert_not_borrowed!(self);
        if occupied == MapCellState::Borrowed {
            return None;
        }
        self.occupied.set(MapCellState::Init);

        // SAFETY:
        // - Since `occupied` is `Init` or `Uninit`, no `&mut` to the `val` exists, meaning it
        //   is safe to mutate the `get` pointer.
        // - If occupied is `Init`, `maybe_uninit_val` must be initialized.
        let maybe_uninit_val = unsafe { self.val.get().replace(MaybeUninit::new(val)) };
        (occupied == MapCellState::Init).then(|| unsafe { maybe_uninit_val.assume_init() })
    }

    /// Calls `closure` with a `&mut` of the contents of the `MapCell`, if available.
    ///
    /// The closure is only called if the `MapCell` has a value.
    /// The state of the `MapCell` is unchanged after the closure completes.
    ///
    /// # Re-entrancy
    ///
    /// This borrows the contents of the cell while the closure is executing.
    /// Be careful about calling methods on `&self` inside of that closure!
    /// To save size, this has no effect in release mode, but if debug assertions
    /// are enabled, this panics to indicate a likely bug.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tock_cells::map_cell::MapCell;
    /// let cell = MapCell::new(1234);
    /// let x = &cell;
    /// let y = &cell;
    ///
    /// x.map(|value| {
    ///     // We have mutable access to the value while in the closure
    ///     *value += 1;
    /// });
    ///
    /// // After the closure completes, the mutable memory is still in the cell,
    /// // but potentially changed.
    /// assert_eq!(y.take(), Some(1235));
    /// ```
    ///
    /// # Panics
    /// If debug assertions are enabled, this panics if the `MapCell`'s contents are already borrowed.
    #[inline(always)]
    pub fn map<F, R>(&self, closure: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> R,
    {
        debug_assert_not_borrowed!(self);
        (self.occupied.get() == MapCellState::Init).then(move || {
            self.occupied.set(MapCellState::Borrowed);
            // `occupied` is reset to initialized at the end of scope,
            // even if a panic occurs in `closure`.
            struct ResetToInit<'a>(&'a Cell<MapCellState>);
            impl Drop for ResetToInit<'_> {
                #[inline(always)]
                fn drop(&mut self) {
                    self.0.set(MapCellState::Init);
                }
            }
            let _reset_to_init = ResetToInit(&self.occupied);
            unsafe { closure(&mut *self.val.get().cast::<T>()) }
        })
    }

    /// Behaves like `map`, but returns `default` if there is no value present.
    #[inline(always)]
    pub fn map_or<F, R>(&self, default: R, closure: F) -> R
    where
        F: FnOnce(&mut T) -> R,
    {
        self.map(closure).unwrap_or(default)
    }

    /// Behaves the same as `map`, except the closure is allowed to return
    /// an `Option`.
    #[inline(always)]
    pub fn and_then<F, R>(&self, closure: F) -> Option<R>
    where
        F: FnOnce(&mut T) -> Option<R>,
    {
        self.map(closure).flatten()
    }

    /// If a value is present `modify` is called with a borrow.
    /// Otherwise, the value is set with `G`.
    #[inline(always)]
    pub fn modify_or_replace<F, G>(&self, modify: F, mkval: G)
    where
        F: FnOnce(&mut T),
        G: FnOnce() -> T,
    {
        if self.map(modify).is_none() {
            self.put(mkval());
        }
    }
}

// Licensed under the Apache License, Version 2.0 or the MIT License.
// SPDX-License-Identifier: Apache-2.0 OR MIT
// Copyright Tock Contributors 2022.

//  `OptionalCell` convenience type

// use core::cell::Cell;

/// `OptionalCell` is a `Cell` that wraps an `Option`. This is helper type
/// that makes keeping types that can be `None` a little cleaner.
pub struct OptionalCell<T> {
    value: Cell<Option<T>>,
}

impl<T> OptionalCell<T> {
    /// Create a new OptionalCell.
    pub const fn new(val: T) -> OptionalCell<T> {
        OptionalCell {
            value: Cell::new(Some(val)),
        }
    }

    /// Create an empty `OptionalCell` (contains just `None`).
    pub const fn empty() -> OptionalCell<T> {
        OptionalCell {
            value: Cell::new(None),
        }
    }

    /// Update the stored value.
    pub fn set(&self, val: T) {
        self.value.set(Some(val));
    }

    /// Insert the value of the supplied `Option`, or `None` if the supplied
    /// `Option` is `None`.
    pub fn insert(&self, opt: Option<T>) {
        self.value.set(opt);
    }

    /// Replace the contents with the supplied value.
    /// If the cell was not empty, the previous value is returned, otherwise
    /// `None` is returned.
    pub fn replace(&self, val: T) -> Option<T> {
        let prev = self.take();
        self.set(val);
        prev
    }

    /// Reset the stored value to `None`.
    pub fn clear(&self) {
        self.value.set(None);
    }

    /// Check if the cell contains something.
    pub fn is_some(&self) -> bool {
        let value = self.value.take();
        let out = value.is_some();
        self.value.set(value);
        out
    }

    /// Check if the cell is None.
    pub fn is_none(&self) -> bool {
        let value = self.value.take();
        let out = value.is_none();
        self.value.set(value);
        out
    }

    /// Returns true if the option is a Some value containing the given value.
    pub fn contains(&self, x: &T) -> bool
    where
        T: PartialEq,
    {
        let value = self.value.take();
        let out = match &value {
            Some(y) => y == x,
            None => false,
        };
        self.value.set(value);
        out
    }

    /// Transforms the contained `Option<T>` into a `Result<T, E>`, mapping
    /// `Some(v)` to `Ok(v)` and `None` to `Err(err)`.
    ///
    /// Arguments passed to `ok_or` are eagerly evaluated; if you are passing
    /// the result of a function call, it is recommended to use `ok_or_else`,
    /// which is lazily evaluated.
    pub fn ok_or<E>(self, err: E) -> Result<T, E> {
        self.value.into_inner().ok_or(err)
    }

    /// Transforms the contained `Option<T>` into a `Result<T, E>`, mapping
    /// `Some(v)` to `Ok(v)` and `None` to `Err(err)`.
    pub fn ok_or_else<E, F>(self, err: F) -> Result<T, E>
    where
        F: FnOnce() -> E,
    {
        self.value.into_inner().ok_or_else(err)
    }

    /// Returns `None` if the option is `None`, otherwise returns `optb`.
    pub fn and<U>(self, optb: Option<U>) -> Option<U> {
        self.value.into_inner().and(optb)
    }

    /// Returns `None` if the option is `None`, otherwise calls `predicate` with
    /// the wrapped value and returns:
    ///
    /// - `Some(t)` if `predicate` returns `true` (where `t` is the wrapped value), and
    /// - `None` if `predicate` returns `false`.
    pub fn filter<P>(self, predicate: P) -> Option<T>
    where
        P: FnOnce(&T) -> bool,
    {
        self.value.into_inner().filter(predicate)
    }

    /// Returns the option if it contains a value, otherwise returns `optb`.
    ///
    /// Arguments passed to or are eagerly evaluated; if you are passing the
    /// result of a function call, it is recommended to use `or_else`, which
    /// is lazily evaluated.
    pub fn or(self, optb: Option<T>) -> Option<T> {
        self.value.into_inner().or(optb)
    }

    /// Returns the option if it contains a value, otherwise calls `f` and
    /// returns the result.
    pub fn or_else<F>(self, f: F) -> Option<T>
    where
        F: FnOnce() -> Option<T>,
    {
        self.value.into_inner().or_else(f)
    }

    /// Return the contained value and replace it with None.
    pub fn take(&self) -> Option<T> {
        self.value.take()
    }

    /// Returns the contained value or a default
    ///
    /// Consumes the `self` argument then, if `Some`, returns the contained
    /// value, otherwise if `None`, returns the default value for that type.
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        self.value.into_inner().unwrap_or_default()
    }
}

impl<T: Copy> OptionalCell<T> {
    /// Returns a copy of the contained [`Option`].
    //
    // This was originally introduced in PR #2531 [1], then renamed to `extract`
    // in PR #2533 [2], and finally renamed back in PR #3536 [3].
    //
    // The rationale for including a `get` method is to allow developers to
    // treat an `OptionalCell<T>` as what it is underneath: a `Cell<Option<T>>`.
    // This is useful to be interoperable with APIs that take an `Option<T>`, or
    // to use an *if-let* or *match* expression to perform case-analysis on the
    // `OptionalCell`'s state: this avoids using a closure and can thus allow
    // Rust to deduce that only a single branch will ever be entered (either the
    // `Some(_)` or `None`) branch, avoiding lifetime & move restrictions.
    //
    // However, there was pushback for that name, as an `OptionalCell`'s `get`
    // method might indicate that it should directly return a `T` -- given that
    // `OptionalCell<T>` presents itself as to be a wrapper around
    // `T`. Furthermore, adding `.get()` might have developers use
    // `.get().map(...)` instead, which defeats the purpose of having the
    // `OptionalCell` convenience wrapper in the first place. For these reasons,
    // `get` was renamed to `extract`.
    //
    // Unfortunately, `extract` turned out to be a confusing name, as it is not
    // an idiomatic method name as found on Rust's standard library types, and
    // further suggests that it actually removes a value from the `OptionalCell`
    // (as the `take` method does). Thus, it has been renamed back to `get`.
    //
    // [1]: https://github.com/tock/tock/pull/2531
    // [2]: https://github.com/tock/tock/pull/2533
    // [3]: https://github.com/tock/tock/pull/3536
    pub fn get(&self) -> Option<T> {
        self.value.get()
    }

    /// Returns the contained value or panics if contents is `None`.
    /// We do not use the traditional name for this function -- `unwrap()`
    /// -- because the Tock kernel discourages panicking, and this name
    /// is intended to discourage users from casually adding calls to
    /// `unwrap()` without careful consideration.
    #[track_caller]
    pub fn unwrap_or_panic(&self) -> T {
        self.value.get().unwrap()
    }

    /// Returns the contained value or a default.
    pub fn unwrap_or(&self, default: T) -> T {
        self.value.get().unwrap_or(default)
    }

    /// Returns the contained value or computes a default.
    pub fn unwrap_or_else<F>(&self, default: F) -> T
    where
        F: FnOnce() -> T,
    {
        self.value.get().unwrap_or_else(default)
    }

    /// Call a closure on the value if the value exists.
    pub fn map<F, R>(&self, closure: F) -> Option<R>
    where
        F: FnOnce(T) -> R,
    {
        self.value.get().map(|val| closure(val))
    }

    /// Call a closure on the value if the value exists, or return the
    /// default if the value is `None`.
    pub fn map_or<F, R>(&self, default: R, closure: F) -> R
    where
        F: FnOnce(T) -> R,
    {
        self.value.get().map_or(default, |val| closure(val))
    }

    /// If the cell contains a value, call a closure supplied with the
    /// value of the cell. If the cell contains `None`, call the other
    /// closure to return a default value.
    pub fn map_or_else<U, D, F>(&self, default: D, closure: F) -> U
    where
        D: FnOnce() -> U,
        F: FnOnce(T) -> U,
    {
        self.value.get().map_or_else(default, |val| closure(val))
    }

    /// If the cell is empty, return `None`. Otherwise, call a closure
    /// with the value of the cell and return the result.
    pub fn and_then<U, F: FnOnce(T) -> Option<U>>(&self, f: F) -> Option<U> {
        self.value.get().and_then(f)
    }
}

// Manual implementation of the [`Default`] trait, as
// `#[derive(Default)]` incorrectly constraints `T: Default`.
impl<T> Default for OptionalCell<T> {
    /// Returns an empty [`OptionalCell`].
    fn default() -> Self {
        OptionalCell::empty()
    }
}
