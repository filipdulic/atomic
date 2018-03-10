// trait SharedPointer {}
//
// impl<T: Pointer> Deref for HazardGuard<T> {
//     type Target = <T as Pointer>::GuardTarget;
//
//     fn deref(&self) -> &Self::Target {
//     }
// }
//
// impl<T> Pointer for Option<Box<T>> {
//     type GuardTarget = Option<SharedBox<T>>;
// }
//
// impl<T> Pointer for Option<Arc<T>> {
//     type GuardTarget = Option<Arc<T>>;
// }
//
// struct HazardGuard<T: Pointer> {
//     inner: T::GuardTarget,
// }
//
// // TODO: assertaj size_of da je 1 word
// // TODO: inlineaj methode pointera
// // TODO: fn try_unwrap(this: HazardGuard<T>) -> Result<T, HazardGuard<T>>
// // TODO: fn try_unwrap(this: HazardCell<T>) -> Result<T, HazardCell<T>>
