# Unsoundness in [`owning_ref`]

This article is about the unsound api which I found in [`owning_ref`]. [`Owning_ref`] is a library that has 11 million all-time downloads and 60 reverse dependencies. In this article I will introduce [`owning_ref`], explain the problems and discuss solving them.

## tl;dr and some links
I found a few unsound functions in the [`owning_ref`] library. Two of them have been found in the past but weren't fixed. The issue is [here](https://github.com/Kimundi/owning-ref-rs/issues/77) and my fork fixing the problems is [here](https://github.com/noamtashma/owning-ref-rs/tree/fix_unsound_map) with a pull request [here](https://github.com/Kimundi/owning-ref-rs/pull/78). The current status regarding unsafety in [`owning_ref`] is detailed [here](#previous-status). The examples of unsoundness can be found in this repository in the `src` folder and are runnable by `cargo run`.

## Introduction to [`owning_ref`]
This section is an introduction to [`owning_ref`]. If you already know the library, you can skip it.
[`Owning_ref`] is a nice library with a very cool idea. It allows you to keep a reference bundled together with its owner. This means that like an owned value, it can have an unlimited lifetime. And like a reference, it can be processed without reallocation.

In the next section I will go over the most important functions from the library that are needed in order to understand the article. In order to learn more, here's [`owning_ref`]'s documentation.

### Reading the api
More specifically, an owning reference of type [`OwningRef`]`<O, T>` consists of an owned value of type `O` and a reference of type `&T`. There's also a version for a unique reference type `&mut T` which is [`OwningRefMut`]`<O, T>`.
First of all, since these types represent references, they implement `Deref` and `DerefMut`:
```rust
impl<O, T: ?Sized> Deref for OwningRef<O, T>
impl<O, T: ?Sized> Deref for OwningRefMut<O, T>
impl<O, T: ?Sized> DerefMut for OwningRefMut<O, T>
```

By "processing" in the introduction section I mean that you can call map-like methods, like [`Ref::map`](https://doc.rust-lang.org/std/cell/struct.Ref.html#method.map), that have these signatures:

```rust
impl OwningRef<O, T> {
  pub fn map<F, U: ?Sized>(self, f: F) -> OwningRef<O, U>
    where
        O: StableAddress,
        F: FnOnce(&T) -> &U, 

  pub fn map_with_owner<F, U: ?Sized>(self, f: F) -> OwningRef<O, U>
    where
        O: StableAddress,
        F: for<'a> FnOnce(&'a O, &'a T) -> &'a U, 
}
```
And mutable counterparts:
```rust
impl OwningRefMut<O, T> {
  pub fn map<F, U: ?Sized>(self, f: F) -> OwningRef<O, U>
    where
        O: StableAddress,
        F: FnOnce(&mut T) -> &U, 

  pub fn map_mut<F, U: ?Sized>(self, f: F) -> OwningRefMut<O, U>
    where
        O: StableAddress,
        F: FnOnce(&mut T) -> &mut U,
}
```

[`StableAddress`] is an unsafe marker trait that encapsulates the following idea: when our owning reference moves, our owner moves with it. This usually invalidates the original reference that was borrowed from the owner. However, we want it to stay valid. For example, we know that when a `Box` is moved, its inner value is still at the same place. So, a trait [`StableAddress`] encapsulates this guarantee.

(This currently conflicts with StackedBorrows, and thus optimizations might change the meaning of code because of this. However, this is outside the scope of the article)

`O: `[`StableAddress`] is required when creating new owning references, and the owner is immediately dereferenced on creation, to prevent the reference from pointing directly to the owner. These are the signatures:

```rust
impl OwningRef<O, T> {
  // Create a new `OwningRef`. Requires `O: StableAddress` to ensure that moving the owner
  // doesn't invalidate the reference.
  pub fn new(o: O) -> Self
    where
        O: StableAddress,
        O: Deref<Target = T>, 
}

impl OwningRefMut<O, T> {
  // Create a new `OwningRef`. Requires `O: StableAddress` to ensure that moving the owner
  // doesn't invalidate the reference.
  pub fn new(o: O) -> Self
    where
        O: StableAddress,
        O: DerefMut<Target = T>, 
}
```

In addition, here are a few more important methods from [`owning_ref`], which access the owner. Their signatures are: 

```rust
impl OwningRef<O, T> {
  pub fn as_owner(&self) -> &O

  pub fn into_owner(self) -> O
}
```

And mutable counterparts:
```rust
impl OwningRefMut<O, T> {
  pub fn as_owner(&self) -> &O

  pub fn as_owner_mut(&mut self) -> &mut O

  pub fn into_owner(self) -> O
}
```

## Unstable Address
[`map_with_owner`] is weird. The callback's signature is
`FnOnce(&O, &T) -> &U`. What if I just give back the owner's reference? The owner could just be moved away.

```rust
fn unstable_address() {
    println!("unstable_address");
    let ow_ref = OwningRef::new(Box::new(5));
    let new_ow_ref = ow_ref.map_with_owner(|owner, _my_ref| owner);
    println!("Reading memory that was moved from: {}", *new_ow_ref);
}
```
After all of that hard work to prevent the reference from pointing to the owner by requiring
`O: `[`StableAddress`]!

## Reading the owner
The first thing that occured to me was this: How come we can change the owner while we still have a unique reference into the owner? Even though we can't access the reference at the exact same time we're mutating the owner, this is still sketchy. And sure enough, it's unsound. One way to show unsoundness is to cause the owner to deallocate, so that the reference now points to invalid memory.

```rust
// Original credit: [https://github.com/comex/owning_ref_bug/blob/master/src/main.rs]
fn ref_mut_as_owner_mut() {
    println!("ref_mut_as_owner_mut");
    let mut ow_ref = OwningRefMut::new(Box::new(5));
    *ow_ref.as_owner_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *ow_ref);
}
```
It turns out this unsoundness was already found beforehand (but not fixed yet).

So we've determined [`OwningRefMut`]`::`[`as_owner_mut`] is unsound. Is removing it enough? Well, say we try to use [`OwningRefMut`]`::`[`as_owner`][0] instead. It's only a shared reference to the owner. But it's "aliasing" a mutable reference, so it's still sketchy. Let's try to do the same thing: use the shared reference to the owner in order to deallocate whatever our reference is pointing at. We want a shared reference to the owner to deallocate something. Sounds like a case for interior mutability:
```rust
fn ref_mut_as_owner_failed() {
    use core::cell::RefCell;
    println!("ref_mut_as_owner");
    let ow_ref = OwningRefMut::new(Box::new(RefCell::new(Box::new(5))));
    let new_ow_ref = ow_ref.map_mut(|cell| &mut cell.borrow_mut());
    // We could call `as_owner_mut`, but we already saw that `as_owner_mut` is inherently unsound.
    *new_ow_ref.as_owner().borrow_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *new_ow_ref);
}
```

So that doesn't work, since we can't manage to convince [`OwningRefMut`]`::map_mut` to move our reference into the `RefCell` instead of pointing at it from the outside...
However, inside the map, we have unique access to the cell, which allows us to use [`RefCell::get_mut`]: 

```rust
// Original credit: [https://github.com/comex/owning_ref_bug/blob/master/src/main.rs]
fn ref_mut_as_owner() {
    use core::cell::RefCell; // `Cell` and any other kind of cell also works
    println!("ref_mut_as_owner");
    let ow_ref = OwningRefMut::new(Box::new(RefCell::new(Box::new(5))));
    let new_ow_ref = ow_ref.map_mut(|cell| &mut **cell.get_mut());
    // We could call `as_owner_mut`, but we already saw that `as_owner_mut` is inherently unsound.
    *new_ow_ref.as_owner().borrow_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *new_ow_ref);
}
```
This unsoundness was also already found beforehand (but not fixed yet). But now we get to more unsoundness that hasn't been found yet.

Let's say we also rule out the [`OwningRefMut`]`::`[`as_owner`][0] method as inherently unsound. Is there any other way we can exploit this?
We can try to use the [`OwningRef`]`::`[`as_owner`][1] method instead:

```rust
fn ref_mut_to_ref() {
    use core::cell::RefCell; // `Cell` and any other kind of cell also works
    println!("ref_mut_to_ref");
    let ow_ref = OwningRefMut::new(Box::new(RefCell::new(Box::new(5))));
    let new_ow_ref = ow_ref.map(|cell| &**cell.get_mut());
    // We could call `as_owner_mut`, but we already saw that `as_owner_mut` is inherently unsound.
    // Have to convert to `OwningRef` In order to use `OwningRef::as_owner`
    // instead of `OwningRefMut::as_owner`.
    *new_ow_ref.as_owner().borrow_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *new_ow_ref);
}
```

In addition, another method that enables reading the owner is [`OwningRef`]`::`[`map_with_owner`],
which doesn't have any equivalent for [`OwningRefMut`] (I wonder why?). It goes like this:

```rust
fn ref_mut_to_ref_2() {
    use core::cell::RefCell; // `Cell` and any other kind of cell also works
    println!("ref_mut_to_ref_2");
    let ow_ref = OwningRefMut::new(Box::new(RefCell::new(0)));
    let new_ow_ref = ow_ref.map(|cell| &mut *cell.get_mut());
    // We could call `as_owner_mut`, but we already saw that `as_owner_mut` is inherently unsound.
    // Have to convert to `OwningRef` In order to use `OwningRef::map_with_owner`.
    new_ow_ref.map_with_owner(|owner, x| {
        // Now we have both mutable and immutable access to the same value
        println!("First read of reference {}", x);
        *owner.borrow_mut() = 1;
        println!("Second read of reference {}", x);
        x
    });
}
```

Now we don't have to do a deallocation to show definite unsoundness, because we have a unique reference that's definitely aliasing another reference.

## Safety discussion
So, what do we do now? What is and isn't safe? How can we fix this?

First of all, fixing the problems described in this library won't make [`owning_ref`] completely clean and safe. This is because of this [Conflict with StackedBorrows' rules](https://github.com/Kimundi/owning-ref-rs/issues/49) that might cause your program to change meaning when compiled with optimizations. However, this is out of the scope of this article: this is about plain unsoundness, no optimization and tricky language semantics needed.

### Unstable address
The "unstable address" problem by itself can be fixed quite easily. Just don't give out a reference to the owner! I would be surprised if anyone really needed a direct reference to the owner itself in order to `map` their reference, instead of just using whatever the owner was pointing at. The owner is required to be a `Deref` value for the [`owning_ref`] to be created in the first place. So, just replace the old function with this new version:
```rust
pub fn map_with_owner<F, U: ?Sized>(self, f: F) -> OwningRef<'t, O, U>
    where O: StableAddress + Deref,
        F: for<'a> FnOnce(&'a O::Target, &'a T) -> &'a U
```
And now the new reference can't point to the owner anymore.

### `OwningRefMut::{as_owner, as_owner_mut}`
The [`OwningRefMut`]`::{`[`as_owner`][0]`, `[`as_owner_mut`]`}` problems are more difficult, but in the end, straightforward. [`OwningRefMut`]`<O, T>` means it contains a unique reference that (may) borrow from the owner. That means that nothing else can access the owner. That's it. There's no workaround. The solution has to be removing these two functions.

### `OwningRef::as_owner`
So how come we can cause the same unsoundness using [`OwningRef`]`::`[`as_owner`][1]? [`OwningRef`]`<O, T>` has an invariant: it contains a shared reference that may only borrow immutably from the owner. So clearly, we can share the owner too. So [`OwningRef`]`::`[`as_owner`][1] and [`OwningRef`]`::`[`map_with_owner`] (the new version of course) are valid according to this invariant. What went wrong?

When we convert from a [`OwningRefMut`]`<O, T>` to an [`OwningRef`]`<O, T>`, we know that that reference might be borrowing <b>mutably</b> from the owner. Converting to an [`OwningRef`] necessitates converting our reference from mutable to immutable. Intuitively, this ensures the reference now only immutably borrows from the owner, which is necessary to comply with [`OwningRef`]'s invariant.

But that isn't true! This is the heart of the issue. Say we do
```rust
use std::cell::RefCell;
let mut o = RefCell::new(7);
let immutable_ref : &i32 = &*(&mut o).get_mut();
&o; // read from the owner
&immutable_ref; // ensure `immutable_ref` is still live here
```
Which is basically equivalent to the unsoundness examples.
`immutable_ref` was created through `&mut o`. Even though it's an immutable reference, it still borrows mutably from `o`. And indeed, rust complains that this code is unsafe.

The [`OwningRef`]`::`[`as_owner`][1], [`OwningRef`]`::`[`map_with_owner`] functions assume that the owner may <b>only</b> be shared. But the conversions from [`OwningRefMut`] into [`OwningRef`] imply that the reference borrows from the owner mutably. And these two assumptions contradict each other.

## Three options
I can see three options for fixing this unsoundness:
1.   Disallow accessing the owner for [`OwningRef`] as well, but keep the conversions.
2.   Allow acessing the owner for [`OwningRef`], but disallow conversions (The conversions are the [`OwningRef`]`<O, T>: From<OwningRefMut<O, T>>` instance that is too strict anyway, and [`OwningRefMut]`::{map, try_map}`).
3.   Weird compromise: Have two distinct types for [`OwningRef`] depending on if the owner is mutably borrowed.

The question is really about the meaning of [`OwningRef`]. The first option says, "The reference in [`OwningRef`] may borrow from the owner either immutably or mutably". The second option says "The reference in [`OwningRef`] may only borrow from the owner immutably". Really, these two interpretations are _two distinct types_, and the third option says, "These are both possible, let's duplicate all of our code twice just in case".

## My fork
I've created a fork to create a version that fixes these problems.
First of all, I'm building on [this](https://github.com/Kimundi/owning-ref-rs/pull/72) pull request, that already solves the problems that were already found: it removes [`OwningRefMut`]`::{`[`as_owner`][0]`, `[`as_owner_mut`]`}`, and adds a lifetime to fix the [tricky existing unsoundness in map](https://github.com/Kimundi/owning-ref-rs/issues/71).

I've decided to choose the second option: [`OwningRef`]`::`[`as_owner`][1], and disallow converting from [`OwningRefMut`] to [`OwningRef`]. In order to preserve backwards compatibility as much as possible, I've kept the old functions as unsafe, `#[deprecated]` functions, although I'm not sure if that's really necessary.

## Previous status
Some unsoundness issues in [`owning_ref`] were already found in the past. These were fixed:
* [This](https://github.com/Kimundi/owning-ref-rs/commit/87b48117bd71f5e1e111484796e76be6953b27f1) seems to
  imply that some kind of unsoundness was at play, but I didn't find the original issue, so I'm not sure.
* [Incorrect Send/Sync implementations](https://github.com/Kimundi/owning-ref-rs/issues/26)

This is the original finding of the [`OwningRefMut`]`::{`[`as_owner`][0]`, `[`as_owner_mut`]`}` unsoundness that I rediscovered:
* [`OwningRefMut::{as_owner, as_owner_mut}` are unsound](https://github.com/Kimundi/owning-ref-rs/issues/61)

which isn't fixed at the time of this writing.

And these are also not fixed at the time of this writing:
* [Conflict with StackedBorrows' rules](https://github.com/Kimundi/owning-ref-rs/issues/49) that turns correct code into incorrect code when optimizing. It seems impossible or very hard to fix without language-level intervention, and it's uncertain if Rust should intervene.
* [`map` is unsound](https://github.com/Kimundi/owning-ref-rs/issues/71) because of very tricky implicit bounds.

And also, this is the [First claim I found](https://github.com/Kimundi/owning-ref-rs/issues/48) that [`OwningRefMut`]`::`[`as_owner_mut`] is unsound, although the given example wasn't really an example of unsoundness. The issue was ignored for some time and then the OP closed his own issue.

## Conclusion
In conclusion:
* [`Owning_ref`] is a cool library that implements a useful Rust primitive. It's popular: it has 11 million all-time downloads and 60 reverse dependencies. It's also used in rustc as part of the [rustc data structures library](https://doc.rust-lang.org/beta/nightly-rustc/rustc_data_structures/owning_ref/index.html).

* Writing safe rust primitives is _hard_. We in the Rust community should encourage developers to check libraries for safety and review each other's code. I welcome feedback on [my own rust primitive library](https://github.com/noamtashma/recursive_reference).

* Users of this library (and all Rust libraries) rely on its safety. If safety issues are uncovered it's important to give them the attention they deserve and make sure they get fixed. Safety problems in[`Owning_ref`] were discovered almost 2 years ago but have yet to be fixed. 

* I submitted a [PR](https://github.com/Kimundi/owning-ref-rs/pull/78) which is based on [this fork](https://github.com/steffahn/owning-ref-rs) and fixes the issues I discussed here. I hope this will help the maintainers of [`OwningRef`] implement a solution.

[`owning_ref`]: https://docs.rs/owning_ref/latest/owning_ref/index.html
[`OwningRef`]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRef.html
[`OwningRefMut`]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRefMut.html
[`RefCell::get_mut`]: https://doc.rust-lang.org/std/cell/struct.RefCell.html#method.get_mut
[0]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRefMut.html#method.as_owner
[1]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRef.html#method.as_owner
[`as_owner_mut`]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRefMut.html#method.as_owner_mut
[`map_with_owner`]: https://docs.rs/owning_ref/latest/owning_ref/struct.OwningRef.html#method.map_with_owner
[`StableAddress`]: https://docs.rs/stable_deref_trait/latest/stable_deref_trait/trait.StableDeref.html