import { execSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const BASE_URL = "https://raw.githubusercontent.com/ashton045/ejs/tcl";
const TARGET_DIR = join(import.meta.dir, "../solver/ejs");

mkdirSync(TARGET_DIR, { recursive: true });

const files: {
	url: string;
	local: string;
	patch?: (content: string) => string;
}[] = [
	{
		url: `${BASE_URL}/src/types.ts`,
		local: "types.ts",
	},
	{
		url: `${BASE_URL}/src/utils.ts`,
		local: "utils.ts",
		patch: (content: string) => {
			return content
				.replace(
					/structure\.every\(\(value, index\) => matchesStructure\(obj\[index\], value\)\)/g,
					"structure.every((value, index) => {\n        const item = obj[index];\n        return item ? matchesStructure(item, value) : false;\n      })",
				)
				.replace(
					/const haystack = Array\.isArray\(obj\) \? obj : Object\.values\(obj\);/g,
					"const haystack: unknown[] = Array.isArray(obj) ? obj : Object.values(obj);",
				)
				.replace(
					/haystack\.some\(\(el\) => matchesStructure\(el, value\)\)/g,
					"haystack.some((el) => matchesStructure(el as ESTree.Node, value as DeepPartial<ESTree.Node>))",
				)
				.replace(
					/if \(!matchesStructure\(obj\[key as keyof typeof obj\], value\)\) \{/g,
					"if (Array.isArray(obj)) return false;\n      const member = (obj as unknown as Record<string, unknown>)[key];\n      if (!matchesStructure(member as ESTree.Node, value as DeepPartial<ESTree.Node>)) {",
				);
		},
	},
	{
		url: `${BASE_URL}/src/yt/solver/setup.ts`,
		local: "setup.ts",
	},
	{
		url: `${BASE_URL}/src/yt/solver/nsig.ts`,
		local: "nsig.ts",
		patch: (content: string) => {
			return content
				.replace(/from "\.\.\/\.\.\/types\.ts";/g, 'from "./types.ts";')
				.replace(/from "\.\.\/\.\.\/utils\.ts";/g, 'from "./utils.ts";')
				.replace(
					/result = \{ key: k1\.val \^ pool\[0\], isDirect: false \};/g,
					"if (pool[0] !== undefined) {\n          result = { key: k1.val ^ pool[0], isDirect: false };\n          return;\n        }",
				)
				.replace(
					/options\.push\({\n\s*name: name,\n\s*statements: fn\.body\.body,\n\s*}\);/g,
					"options.push({\n        name: name as ESTree.Identifier,\n        statements: fn.body.body,\n      });",
				)
				.replace(
					/matchesStructure\(statements, \{ anykey: \[asdasd\] \}\)/g,
					"matchesStructure(statements, { anykey: [asdasd] } as DeepPartial<ESTree.Node>)",
				);
		},
	},
	{
		url: `${BASE_URL}/src/yt/solver/solvers.ts`,
		local: "solvers.ts",
		patch: (content: string) => {
			return content
				.replace(/from "\.\.\/\.\.\/utils\.ts";/g, 'from "./utils.ts";')
				.replace(/from "\.\/nsig\.ts";/g, 'from "./nsig.ts";')
				.replace(/from "\.\/setup\.ts";/g, 'from "./setup.ts";')
				.replace(
					/import { type ESTree } from "meriyah";/g,
					'import type { ESTree } from "meriyah";',
				)
				.replace(
					/import { type DeepPartial } from "\.\/types\.ts";/g,
					'import type { DeepPartial } from "./types.ts";',
				)
				.replace(/args && args\.length/g, "args?.length")
				.replace(
					/return func\.expression\.callee\.object\.body;/g,
					"const b = func.expression.callee.object.body;\n          if (b) return b;",
				)
				.replace(
					/const block = func\.expression\.callee\.body;\n\s*\/\/ Skip `var window = this;`\n\s*block\.body\.splice\(0, 1\);\n\s*return block;/g,
					"const b = func.expression.callee.body;\n          if (b) {\n            b.body.splice(0, 1);\n            return b;\n          }",
				)
				.replace(
					/throw "unexpected structure";/g,
					'throw new Error("unexpected structure");',
				);
		},
	},
];

console.log("updating ejs solver files from ashton045/ejs:tcl...");

for (const file of files) {
	console.log(`fetching ${file.local}...`);
	const res = await fetch(file.url);
	if (!res.ok) {
		throw new Error(
			`failed fetching ${file.url}: ${res.status} ${res.statusText}`,
		);
	}

	let content = await res.text();
	if (file.patch) {
		content = file.patch(content);
	}

	writeFileSync(join(TARGET_DIR, file.local), content, "utf8");
	console.log(`saved ${file.local}`);
}

console.log("formatting and fixing updated files with Biome...");
execSync("bun run lint:fix", {
	stdio: "inherit",
	cwd: join(import.meta.dir, ".."),
});

console.log("all ejs solver files successfully updated and formatted.");
