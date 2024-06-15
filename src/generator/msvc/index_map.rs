use std::collections::HashMap;

use super::VsProject;
use crate::link_type::LinkPtr;

pub(super) struct IndexMap {
	vec: Vec<VsProject>,
	map: HashMap<LinkPtr, usize>,
}

impl IndexMap {
	pub(super) fn new() -> Self {
		IndexMap { vec: Vec::new(), map: HashMap::new() }
	}
	pub(super) fn contains_key(&self, key: &LinkPtr) -> bool {
		self.map.contains_key(key)
	}
	pub(super) fn get(&self, key: &LinkPtr) -> Option<&VsProject> {
		let index = match self.map.get(key) {
			Some(x) => *x,
			None => return None,
		};
		Some(&self.vec[index])
	}
	pub(super) fn insert(&mut self, key: LinkPtr, val: VsProject) {
		self.map.insert(key.clone(), self.vec.len());
		self.vec.push(val);
	}
	pub(super) fn insert_exe(&mut self, val: VsProject) {
		self.vec.push(val);
	}
	pub(super) fn iter(&self) -> IndexMapIter {
		IndexMapIter { index: 0, map: self }
	}
}

impl<'map> IntoIterator for &'map IndexMap {
	type Item = &'map VsProject;
	type IntoIter = IndexMapIter<'map>;

	fn into_iter(self) -> IndexMapIter<'map> {
		self.iter()
	}
}

pub(super) struct IndexMapIter<'map> {
	index: usize,
	map: &'map IndexMap,
}

impl<'map> Iterator for IndexMapIter<'map> {
	type Item = &'map VsProject;

	fn next(&mut self) -> Option<Self::Item> {
		if self.index == self.map.vec.len() {
			return None;
		}
		let index = self.index;
		self.index += 1;
		Some(&self.map.vec[index])
	}
}

impl<'map> DoubleEndedIterator for IndexMapIter<'map> {
	fn next_back(&mut self) -> Option<Self::Item> {
		if self.index == self.map.vec.len() {
			return None;
		}
		self.index += 1;
		Some(&self.map.vec[self.map.vec.len() - self.index])
	}
}
