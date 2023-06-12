//! Support types and traits for bindings generated by `com-scrape`.
//!
//! [`ComPtr`] and [`ComRef`] are smart pointers for interacting with COM objects (calling methods,
//! casting between interfaces, and managing reference counts). The [`Class`] trait can be used for
//! defining COM classes in Rust, and [`ComWrapper`] is a smart pointer used for instantiating
//! those classes.
//!
//! # Reference counting
//!
//! COM objects are reference-counted. The [`ComPtr`] and [`ComRef`] smart pointers manage this
//! automatically where possible, but the function signatures generated by `com-scrape` still pass
//! COM objects as raw pointers, and care must be taken to handle issues of ownership correctly
//! when converting between [`ComPtr`] or [`ComRef`] and raw pointers at these boundaries.
//!
//! A thorough overview of how to manage reference counts for COM objects in a variety of situations
//! can be found on the ["Rules for Managing Reference Counts"][rules] page in the Microsoft COM
//! documentation, and the documentation for each individual [`ComPtr`] and[`ComRef`] method
//! specifies its effect on an object's reference count. However, the following rules of thumb
//! should suffice in the majority of situations:
//!
//! 1. When passing an interface pointer as a function parameter, use [`ComPtr::as_ptr`] to obtain a
//! raw pointer from a [`ComPtr`], or use [`ComRef::as_ptr`] to obtain a raw pointer from a
//! [`ComRef`].
//!
//! 2. When receiving an interface pointer as the return value of a function (or via an out
//! parameter), always use [`ComPtr::from_raw`] to obtain a [`ComPtr`] from the raw pointer.
//!
//! 3. When receiving an interface pointer as a function parameter, always use
//! [`ComRef::from_raw`] to obtain a [`ComRef`] from the raw pointer. If the received interface
//! pointer will be stored beyond the duration of the current function, use
//! [`ComRef::to_com_ptr`] to upgrade the [`ComRef`] to a [`ComPtr`].
//!
//! 4. When returning an interface pointer from a function (or when returning it via an out
//! parameter), always use [`ComPtr::into_raw`] to obtain a raw pointer from a [`ComPtr`].
//!
//! [rules]: https://learn.microsoft.com/en-us/windows/win32/com/rules-for-managing-reference-counts
//!
//! # Implementing COM interfaces from Rust
//!
//! The [`Class`] trait can be used to define COM classes in Rust, and the [`ComWrapper`] smart
//! pointer can be used to instantiate objects of these classes. To define a COM class, start by
//! defining a Rust type:
//!
//! ```ignore
//! struct MyClass { /* ... */ }
//! ```
//!
//! Then implement the desired interface traits for the type:
//!
//! ```ignore
//! impl ISomeInterfaceTrait for MyClass {
//!     unsafe fn some_method(&self) {
//!         /* ... */
//!     }
//! }
//!
//! impl IAnotherInterfaceTrait for MyClass {
//!     unsafe fn another_method(&self) {
//!         /* ... */
//!     }
//! }
//! ```
//!
//! Finally, implement the [`Class`] trait for the type, specifying the set of COM interfaces as a
//! tuple:
//!
//! ```ignore
//! impl Class for MyClass {
//!     type Interfaces = (ISomeInterface, IAnotherInterface);
//! }
//! ```
//!
//! With these definitions in place, [`ComWrapper`] can be used to instantiate a COM object
//! supporting the above interfaces:
//!
//! ```ignore
//! let my_obj = ComWrapper::new(MyClass);
//!
//! let ptr = my_obj.to_com_ptr::<ISomeInterface>().unwrap();
//! ptr.some_method();
//!
//! let ptr = my_obj.to_com_ptr::<IAnotherInterface>().unwrap();
//! ptr.another_method();
//! ```

mod class;
mod ptr;

#[cfg(test)]
mod tests;

use std::ffi::c_void;

pub use class::{Class, ComWrapper, Construct, Header, InterfaceList, MakeHeader, Wrapper};
pub use ptr::{ComPtr, ComRef, SmartPtr};

/// A 16-byte unique identifier for a COM interface.
pub type Guid = [u8; 16];

/// Implemented by interfaces that derive from the `IUnknown` interface (or an equivalent thereof).
///
/// Represents the base interface that all COM objects must implement and from which all COM
/// interfaces must derive. Corresponds to the `IUnknown` interface. Includes functionality for
/// reference counting ([`add_ref`](Self::add_ref) and [`release`](Self::release) and for
/// dynamically casting between interface types ([`query_interface`](Self::query_interface)).
///
/// All interface types generated by `com-scrape` will implement this trait.
pub trait Unknown {
    /// Checks if an object implements the interface corresponding to the given GUID, and if so,
    /// returns a corresponding interface pointer for the object and increments the object's
    /// reference count.
    unsafe fn query_interface(this: *mut Self, iid: &Guid) -> Option<*mut c_void>;

    /// Increments an object's reference count and returns the resulting count.
    unsafe fn add_ref(this: *mut Self) -> usize;

    /// Decrements an object's reference count and returns the resulting count.
    unsafe fn release(this: *mut Self) -> usize;
}

/// Implemented by all COM interface types.
///
/// # Safety
///
/// If a type `I` implements `Interface`, it must have the same layout as the pointer type
/// `*const I::Vtbl`.
///
/// If `I::inherits(J::IID)` returns `true`, then the layout of `J::Vtbl` must be a prefix of the
/// layout of `I::Vtbl`, i.e. a valid pointer to an instance of `I::Vtbl` must also be valid
/// pointer to an instance of `J::Vtbl`.
pub unsafe trait Interface: Unknown {
    /// The type of the virtual method table for this interface.
    type Vtbl;

    /// A 16-byte unique identifier ([`Guid`]) for the COM interface represented by this type.
    const IID: Guid;

    /// Returns `true` if this interface transitively inherits from the interface identified by
    /// `iid`.
    ///
    /// Note that this has safety implications; see the top-level documentation for [`Interface`].
    fn inherits(iid: &Guid) -> bool;
}

/// Represents the "is-a" relationship for interfaces.
///
/// If interface `I` implements `Inherits<J>`, it is valid to cast a pointer of type `*mut I` to a
/// pointer of type `*mut J` and to call any of `J`'s methods via that pointer.
///
/// The `Inherits` relation should be reflexive and transitive, i.e. `I: Inherits<I>` should be
/// true for any type `I`, and if `I: Inherits<J>` and `J: Inherits<K>` are true, then
/// `I: Inherits<K>` should also be true. However, this is not a safety requirement.
///
/// # Safety
///
/// [`Interface`] is a supertrait of `Inherits`, so all of `Interface`'s safety requirements also
/// apply to `Inherits`. In particular, if `I` implements `Inherits`, it must have the same layout
/// as the pointer type `*const I::Vtbl`.
///
/// If `I` implements `Inherits<J>`, then the layout of `J::Vtbl` must be a prefix of the layout of
/// `I::Vtbl`, i.e. a valid pointer to an instance of `I::Vtbl` must also be a valid pointer to an
/// instance of `J::Vtbl`.
pub unsafe trait Inherits<I: Interface>: Interface {}

unsafe impl<I: Interface> Inherits<I> for I {}
