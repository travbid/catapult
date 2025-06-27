use core::{cmp, hash};
use std::{borrow::Borrow, collections::HashMap};

pub(crate) struct IndexMap<K, V>
where
	K: cmp::Eq + hash::Hash,
{
	vec: Vec<(K, V)>,
	map: HashMap<K, usize>,
}

impl<K, V> IndexMap<K, V>
where
	K: cmp::Eq + hash::Hash + Clone,
{
	pub fn new() -> Self {
		IndexMap { vec: Vec::new(), map: HashMap::new() }
	}
	pub fn contains_key<Q>(&self, key: &Q) -> bool
	where
		K: Borrow<Q>,
		Q: ?Sized + hash::Hash + Eq,
	{
		self.map.contains_key(key)
	}
	pub fn get<Q>(&self, key: &Q) -> Option<&V>
	where
		K: Borrow<Q>,
		Q: ?Sized + hash::Hash + Eq,
	{
		let index = match self.map.get(key) {
			Some(x) => *x,
			None => return None,
		};
		Some(&self.vec[index].1)
	}
	pub fn get_mut<Q>(&mut self, key: &Q) -> Option<&mut V>
	where
		K: Borrow<Q>,
		Q: ?Sized + hash::Hash + Eq,
	{
		let index = match self.map.get(key) {
			Some(x) => *x,
			None => return None,
		};
		Some(&mut self.vec[index].1)
	}
	pub fn insert(&mut self, key: K, val: V) {
		if let Some(idx) = self.map.get(&key) {
			self.vec[*idx] = (key, val);
		} else {
			self.map.insert(key.clone(), self.vec.len());
			self.vec.push((key, val));
		}
	}
	// pub(super) fn insert_exe(&mut self, val: VsProject) {
	// 	self.vec.push(val);
	// }
	pub fn iter(&self) -> core::slice::Iter<(K, V)> {
		self.vec.iter()
	}
	pub fn into_values(self) -> impl Iterator<Item = V> {
		self.into_iter().map(|kv| kv.1)
	}
}

impl<'map, K, V> IntoIterator for IndexMap<K, V>
where
	K: cmp::Eq + hash::Hash + Clone,
{
	type Item = (K, V);
	type IntoIter = std::vec::IntoIter<(K, V)>;

	fn into_iter(self) -> Self::IntoIter {
		self.vec.into_iter()
	}
}

impl<'map, K, V> IntoIterator for &'map IndexMap<K, V>
where
	K: cmp::Eq + hash::Hash + Clone,
{
	type Item = &'map (K, V);
	type IntoIter = core::slice::Iter<'map, (K, V)>;

	fn into_iter(self) -> Self::IntoIter {
		self.vec.iter()
	}
}

// pub(crate) struct IndexMapIter<'map, K, V>
// where
// 	K: cmp::Eq + hash::Hash,
// {
// 	index: usize,
// 	map: &'map IndexMap<K, V>,
// }

// impl<'map, K, V> Iterator for IndexMapIter<'map, K, V>
// where
// 	K: cmp::Eq + hash::Hash,
// {
// 	type Item = (&'map K, &'map V);

// 	fn next(&mut self) -> Option<Self::Item> {
// 		if self.index == self.map.vec.len() {
// 			return None;
// 		}
// 		let index = self.index;
// 		self.index += 1;
// 		Some((&self.map.vec[index].0, &self.map.vec[index].1))
// 	}
// }

// impl<'map, K, V> DoubleEndedIterator for IndexMapIter<'map, K, V>
// where
// 	K: cmp::Eq + hash::Hash,
// {
// 	fn next_back(&mut self) -> Option<Self::Item> {
// 		if self.index == self.map.vec.len() {
// 			return None;
// 		}
// 		self.index += 1;
// 		Some((&self.map.vec[self.map.vec.len() - self.index].0, &self.map.vec[self.map.vec.len() - self.index].1))
// 	}
// }
