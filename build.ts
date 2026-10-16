const outfile = process.argv[2];

const buildOptions: Parameters<typeof Bun.build>[0] = {
	entrypoints: ["solver/index.ts"],
	format: "iife",
	minify: true,
};

if (outfile) {
	buildOptions.naming = outfile;
}

const result = await Bun.build(buildOptions);

if (!result.success) {
	for (const message of result.logs) {
		console.error(message);
	}
	process.exit(1);
}

if (outfile && result.outputs.length > 0) {
	const output = result.outputs[0];
	if (output) {
		await Bun.write(outfile, output);
	}
}

export {};
