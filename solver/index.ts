import { type ESTree, parse } from "meriyah";
import { setupDom } from "./botguard/dom.ts";
import { decodeColdStartToken } from "./botguard/helpers.ts";
import { generatePoToken } from "./botguard/index.ts";
import { extractArray } from "./ejs/nsig.ts";
import { getFromPrepared, preprocessPlayer } from "./ejs/solvers.ts";

interface ExtractedSolvers {
	n?: (val: string) => string;
	sig?: (val: string) => string;
}

const ejsCache = new Map<string, ExtractedSolvers>();

function solveViaEjs(
	player_code: string,
	n_param: string | null,
	sig: string | null,
) {
	let solvers = ejsCache.get(player_code);
	if (!solvers) {
		const preprocessed = preprocessPlayer(player_code);
		const extracted = getFromPrepared(preprocessed);
		if (!extracted) {
			throw new Error("failed extracting ejs solvers");
		}
		solvers = {
			n: extracted.n ?? undefined,
			sig: extracted.sig ?? undefined,
		};
		ejsCache.set(player_code, solvers);
	}

	const resolved_n = n_param && solvers?.n ? solvers.n(n_param) : null;
	const resolved_sig = sig && solvers?.sig ? solvers.sig(sig) : null;
	return { n: resolved_n, sig: resolved_sig };
}

interface TargetFn {
	node: ESTree.Node & { range: [number, number] };
	name: string;
	isDecl: boolean;
}

function hookPlayerScript(code: string): string {
	const ast = parse(code, { ranges: true }) as ESTree.Program;
	const statements = ast.body.filter(
		(s): s is ESTree.Statement =>
			!s.type.startsWith("Export") && !s.type.startsWith("Import"),
	);
	const pool = extractArray(statements);

	const matchHolder: { target: TargetFn | null } = { target: null };

	function isUrlSolverFunction(node: unknown): boolean {
		let hasClone = false;
		let hasSplit = false;
		function walk(n: unknown) {
			if (!n || typeof n !== "object") return;
			const obj = n as Record<string, unknown>;
			if (obj.type === "CallExpression") {
				const callee = obj.callee as Record<string, unknown> | undefined;
				const prop = callee?.property as Record<string, unknown> | undefined;
				if (prop?.name === "split") hasSplit = true;
			}
			if (obj.type === "NewExpression" || obj.type === "CallExpression") {
				const callee = obj.callee as Record<string, unknown> | undefined;
				const name = typeof callee?.name === "string" ? callee.name : undefined;
				if (name && (name.includes("Clone") || name.includes("URL")))
					hasClone = true;
			}
			for (const k of Object.keys(obj)) {
				const child = obj[k];
				if (Array.isArray(child)) child.forEach(walk);
				else if (child && typeof child === "object") walk(child);
			}
		}
		walk(node);
		return hasSplit || hasClone;
	}

	function checkCall(callee: unknown, args: unknown[]): boolean {
		if (!callee || !args || args.length < 2) return false;
		const calleeObj = callee as Record<string, unknown>;
		const propObj = calleeObj.property as Record<string, unknown> | undefined;
		const prop = propObj?.name || propObj?.value;
		if (prop !== "set") return false;

		const arg0 = args[0] as Record<string, unknown>;
		const arg1 = args[1] as Record<string, unknown>;

		if (arg0?.value === "alr" && arg1?.value === "yes") return true;

		if (pool && pool.length >= 3) {
			if (arg0?.type === "BinaryExpression" && arg0.operator === "^") {
				const v0 = arg0.left as Record<string, unknown> | undefined;
				const v1 = arg0.right as Record<string, unknown> | undefined;
				if (
					typeof v0?.value === "number" &&
					typeof v1?.value === "number" &&
					(v0.value ^ v1.value) === pool[1]
				)
					return true;
			}
			if (arg1?.type === "BinaryExpression" && arg1.operator === "^") {
				const v0 = arg1.left as Record<string, unknown> | undefined;
				const v1 = arg1.right as Record<string, unknown> | undefined;
				if (
					typeof v0?.value === "number" &&
					typeof v1?.value === "number" &&
					(v0.value ^ v1.value) === pool[2]
				)
					return true;
			}
		}
		return false;
	}

	function walk(node: unknown, current_fn: TargetFn | null) {
		if (!node || typeof node !== "object") return;
		const obj = node as Record<string, unknown>;

		if (obj.type === "FunctionDeclaration") {
			const id = obj.id as ESTree.Identifier | undefined;
			if (
				id?.name &&
				Array.isArray((obj as unknown as { range?: unknown }).range)
			) {
				current_fn = {
					node: obj as unknown as ESTree.Node & { range: [number, number] },
					name: id.name,
					isDecl: true,
				};
			}
		} else if (obj.type === "VariableDeclarator") {
			const id = obj.id as ESTree.Identifier | undefined;
			const init = obj.init as
				| ((ESTree.FunctionExpression | ESTree.ArrowFunctionExpression) & {
						range?: unknown;
				  })
				| undefined;
			if (
				id?.name &&
				Array.isArray(init?.range) &&
				(init?.type === "FunctionExpression" ||
					init?.type === "ArrowFunctionExpression")
			) {
				current_fn = {
					node: init as unknown as ESTree.Node & { range: [number, number] },
					name: id.name,
					isDecl: false,
				};
			}
		}

		if (
			obj.type === "CallExpression" &&
			checkCall(obj.callee, Array.isArray(obj.arguments) ? obj.arguments : [])
		) {
			if (current_fn && isUrlSolverFunction(current_fn.node)) {
				matchHolder.target = current_fn;
				return;
			}
		}

		for (const k of Object.keys(obj)) {
			if (matchHolder.target) return;
			const child = obj[k];
			if (Array.isArray(child)) {
				for (const item of child) {
					walk(item, current_fn);
					if (matchHolder.target) return;
				}
			} else if (child && typeof child === "object") {
				walk(child, current_fn);
				if (matchHolder.target) return;
			}
		}
	}

	walk(ast, null);

	const finalTarget = matchHolder.target;
	if (finalTarget !== null) {
		const [start, end] = finalTarget.node.range;
		const snippet = code.slice(start, end);
		const name = finalTarget.name;
		if (finalTarget.isDecl) {
			return (
				code.slice(0, start) +
				`var ${name}=(globalThis.__solve_url=${snippet})` +
				code.slice(end)
			);
		}
		return (
			code.slice(0, start) +
			`(globalThis.__solve_url=${snippet})` +
			code.slice(end)
		);
	}

	return code;
}

function solveViaEval(player_code: string, stream_url: string) {
	const domSession = setupDom();
	try {
		const hooked = hookPlayerScript(player_code);
		const evaluateInGlobalScope = new Function(
			"code",
			"return (0, eval)(code);",
		);
		evaluateInGlobalScope(hooked);

		const g = globalThis as Record<string, unknown>;
		const solver = g.__solve_url as ((url: URL) => unknown) | undefined;
		if (typeof solver !== "function") {
			throw new Error("failed hooking player __solve_url function");
		}

		const resolved = solver(new URL(stream_url));
		return resolved ? String(resolved) : stream_url;
	} finally {
		domSession.release();
	}
}

export function solve_cipher(
	player_code: string,
	n_param: string | null,
	sig: string | null,
) {
	try {
		const res = solveViaEjs(player_code, n_param, sig);
		return {
			mode: "ejs",
			n: res.n,
			sig: res.sig,
			resolved_url: null,
		};
	} catch (ejs_err) {
		return {
			mode: "eval_needed",
			n: null,
			sig: null,
			resolved_url: null,
			error: String(ejs_err),
		};
	}
}

export function solve_eval(player_code: string, stream_url: string) {
	const resolved_url = solveViaEval(player_code, stream_url);
	return {
		mode: "eval",
		resolved_url,
	};
}

export async function solve_botguard(
	content_binding: string,
	cold_token = false,
) {
	return await generatePoToken(content_binding, cold_token);
}

export function decode_cold_start(token: string) {
	return decodeColdStartToken(token);
}

(globalThis as Record<string, unknown>).solve_cipher = solve_cipher;
(globalThis as Record<string, unknown>).solve_eval = solve_eval;
(globalThis as Record<string, unknown>).solve_botguard = solve_botguard;
(globalThis as Record<string, unknown>).decode_cold_start = decode_cold_start;
