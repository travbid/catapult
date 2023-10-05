use std::sync::Arc;

use super::starlark_link_target::StarLinkTarget;

pub(crate) fn format_strings(items: &[String]) -> String {
	let mut ret = items
		.iter()
		.map(|x| "\"".to_owned() + x + "\"")
		.collect::<Vec<_>>()
		.join(",\n      ");
	if items.len() > 1 {
		ret = String::from("\n      ") + &ret + ",\n   ";
	}
	ret
}
pub(crate) fn format_link_targets(items: &[Arc<dyn StarLinkTarget>]) -> String {
	if items.is_empty() {
		String::new()
	} else if items.len() == 1 {
		items.first().unwrap().name()
	} else {
		let separator = ",\n    ";
		let mut ret = items
			.iter()
			.map(|x| x.name())
			.fold(String::new(), |acc, x| acc + &x + separator);
		ret.insert_str(0, "\n    ");
		ret.pop();
		ret.pop();
		ret
	}
}
