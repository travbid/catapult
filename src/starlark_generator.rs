use starlark::{
	environment::Module,
	eval::Evaluator,
	values::{
		AllocValue, //
		OwnedFrozenValue,
		UnpackValue,
	},
};

use crate::{
	starlark_context::StarContext, //
	starlark_object_library::StarGeneratorVars,
};

pub(crate) fn eval_vars(func: &OwnedFrozenValue, ctx: StarContext, name: &str) -> Result<StarGeneratorVars, String> {
	let module = Module::new();
	let mut evaluator = Evaluator::new(&module);
	let ctx_val = ctx.alloc_value(evaluator.heap());
	let result_val = match evaluator.eval_function(func.value(), &[ctx_val], &[]) {
		Ok(x) => x,
		Err(e) => return Err(format!("Could not evaluate generator function used in {}: {}", name, e)),
	};
	let generator_vars = match StarGeneratorVars::unpack_value(result_val) {
		Some(x) => x,
		None => return Err(format!("Result of generator function could not be unpacked: {}", name)),
	};
	Ok(generator_vars)
}
