use core::{cmp, hash};

#[derive(Debug)]
pub(crate) struct ThinPtr<T: ?Sized>(pub *const T);

impl<T: ?Sized> cmp::PartialEq for ThinPtr<T> {
	fn eq(&self, other: &ThinPtr<T>) -> bool {
		core::ptr::eq(self.0 as *const (), other.0 as *const ())
	}
}
impl<T: ?Sized> cmp::Eq for ThinPtr<T> {}
impl<T: ?Sized> hash::Hash for ThinPtr<T> {
	fn hash<H>(&self, hasher: &mut H)
	where
		H: std::hash::Hasher,
	{
		(self.0 as *const ()).hash(hasher)
	}
}
impl<T: ?Sized> Clone for ThinPtr<T> {
	fn clone(&self) -> Self {
		ThinPtr(self.0)
	}
}
