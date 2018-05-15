pub unsafe trait ByteEq: Eq {}

macro_rules! impl_primitive {
    ($($t:ty),*) => {
        $(unsafe impl ByteEq for $t {})*
    };
}
impl_primitive!((), bool, char, i8, u8, i16, u16, i32, u32, isize, usize);

macro_rules! impl_tuple {
    ($($i:ident),*) => {
        unsafe impl<T: ByteEq> ByteEq for ($($i),*,) {}
    }
}
impl_tuple!(T);
impl_tuple!(T, T);
impl_tuple!(T, T, T);
impl_tuple!(T, T, T, T);
impl_tuple!(T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T, T, T, T, T);
impl_tuple!(T, T, T, T, T, T, T, T, T, T, T, T);

macro_rules! impl_array {
    ($($len:expr),*) => {
        $(
            unsafe impl<T: ByteEq> ByteEq for [T; $len] {}
        )*
    }
}
impl_array!(
    0, 1, 2, 3, 4, 5, 6, 7, 8,
    9, 10, 11, 12, 13, 14, 15, 16,
    17, 18, 19, 20, 21, 22, 23, 24,
    25, 26, 27, 28, 29, 30, 31, 32
);
