use owning_ref::*;

fn main() {
    ref_mut_as_owner_mut();
    unstable_address();
    ref_mut_as_owner();
    ref_mut_to_ref();
    ref_mut_to_ref_2();
}

// TODO:
// Versions of unsoundness using `Cell` instead of `RefCell`
// Check: is the `Send` implementation of `OwningRef` incorrect? it seems to send a reference to O,
// but doesn't require `O: Sync`.
// Check: can this be used to create unsoundness by sending a `OwningRefMut`,
// converting to `OwningRef`, and then reading the owner?

// Original credit: [https://github.com/comex/owning_ref_bug/blob/master/src/main.rs]
fn ref_mut_as_owner_mut() {
    println!("ref_mut_as_owner_mut");
    let mut ow_ref = OwningRefMut::new(Box::new(5));
    *ow_ref.as_owner_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *ow_ref);
    println!("");
}

fn unstable_address() {
    println!("unstable_address");
    let ow_ref = OwningRef::new(Box::new(5));
    let new_ow_ref = ow_ref.map_with_owner(|owner, _my_ref| owner);
    println!("Reading memory that was moved from: {}", *new_ow_ref);
    println!("");
}

// Original credit: [https://github.com/comex/owning_ref_bug/blob/master/src/main.rs]
fn ref_mut_as_owner() {
    use core::cell::RefCell; // `Cell` and any other kind of cell also works
    println!("ref_mut_as_owner");
    let ow_ref = OwningRefMut::new(Box::new(RefCell::new(Box::new(5))));
    let new_ow_ref = ow_ref.map_mut(|cell| &mut **cell.get_mut());
    // We could call `as_owner_mut`, but we already saw that `as_owner_mut` is inherently unsound.
    *new_ow_ref.as_owner().borrow_mut() = Box::new(9);
    println!("Reading deallocated memory: {}", *new_ow_ref);
    println!("");
}

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
    println!("");
}

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
    println!("");
}

// Credit: [https://github.com/Kimundi/owning-ref-rs/issues/71]
fn tricky_map_unsoundness<'a, T>(input: &'a T) -> &'static T {
    let ow_ref1 = OwningRef::new(Box::new(()));
    let input_ref_ref = &input;
    let ow_ref2: OwningRef<Box<()>, &&T> = ow_ref1.map(|x| &input_ref_ref);
    let ow_ref3: OwningRef<Box<()>, T> = ow_ref2.map(|s| &***s);
    &*Box::leak(Box::new(ow_ref3))
}
