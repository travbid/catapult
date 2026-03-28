use core::{cmp, hash};
use std::{borrow::Borrow, collections::HashMap};

pub(crate) struct IndexSet<T>
where
	T: cmp::Eq + hash::Hash,
{
	vec: Vec<T>,
	map: HashMap<T, usize>,
}

impl<T> IndexSet<T>
where
	T: cmp::Eq + hash::Hash + Clone,
{
	pub fn new() -> Self {
		IndexSet { vec: Vec::new(), map: HashMap::new() }
	}
	pub fn contains_key<Q>(&self, key: &Q) -> bool
	where
		T: Borrow<Q>,
		Q: ?Sized + hash::Hash + Eq,
	{
		self.map.contains_key(key)
	}
	pub fn get<Q>(&self, key: &Q) -> Option<&T>
	where
		T: Borrow<Q>,
		Q: ?Sized + hash::Hash + Eq,
	{
		let index = match self.map.get(key) {
			Some(x) => *x,
			None => return None,
		};
		Some(&self.vec[index])
	}
	pub fn insert(&mut self, val: T) {
		self.map.insert(val.clone(), self.vec.len());
		self.vec.push(val);
	}
	pub fn iter<'a>(&'a self) -> core::slice::Iter<'a, T> {
		self.vec.iter()
	}
}

impl<'set, T> IntoIterator for IndexSet<T>
where
	T: cmp::Eq + hash::Hash + Clone,
{
	type Item = T;
	type IntoIter = std::vec::IntoIter<T>;

	fn into_iter(self) -> Self::IntoIter {
		self.vec.into_iter()
	}
}
